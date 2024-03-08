use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use embedded_graphics::{image::Image, prelude::*};
use futures::StreamExt;
use local_ip_address::{local_ip, local_ipv6};
use mdns_sd::ServiceInfo;
use printer::{Printer, PrinterStatus};
use rppal::gpio::Gpio;
use serde::Deserialize;
use std::{path::PathBuf, process::ExitCode, sync::Arc, time::Duration};
use tokio::{
    select, signal,
    sync::{
        mpsc::{Receiver, UnboundedReceiver},
        RwLock,
    },
};
use zbus::Connection;

use clap::Parser;
use clap_verbosity_flag::Verbosity;

use image::io::Reader as ImageReader;

mod displays;
mod icons;
mod network;
mod printer;

fn get_default_icon_path() -> PathBuf {
    let mut path = std::env::current_exe().unwrap();
    path.pop();
    path.push("icons");
    path
}

/// Akri KubeCon EU 2024 Demo
#[derive(Debug, Parser)]
struct Cli {
    #[command(flatten)]
    verbose: Verbosity,
    #[arg(default_value=get_default_icon_path().into_os_string())]
    icons_path: PathBuf,
}

#[derive(Debug, PartialEq)]
enum Status {
    Play,
    Pause,
    Discard,
}

struct AppState<'a> {
    printer: Printer,
    options: Cli,
    status: RwLock<Status>,
    network: network::NetworkManagerProxy<'a>,
}

#[derive(Deserialize)]
struct PrintParams {
    name: String,
}

const BUTTON_1: u8 = 25;
const BUTTON_2: u8 = 26;

#[derive(Debug)]
enum Button {
    Key1,
    Key2,
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}

async fn print_heart_page(
    State(state): State<Arc<AppState<'_>>>,
    Path(icon): Path<String>,
    Json(payload): Json<PrintParams>,
) -> Result<(), (StatusCode, String)> {
    let image = ImageReader::open(state.options.icons_path.join(format!("{}.png", icon)))
        .map_err(|_| (StatusCode::NOT_FOUND, "Not Found".to_owned()))?
        .decode()
        .map_err(|_| (StatusCode::NOT_FOUND, "Not Found".to_owned()))?
        .resize(256, 256, image::imageops::FilterType::Triangle);
    let heart = ImageReader::open("heart.png")
        .unwrap()
        .decode()
        .unwrap()
        .resize(256, 256, image::imageops::FilterType::Triangle);

    let name_size = payload.name.len();
    if name_size > 12 {
        return Err((StatusCode::BAD_REQUEST, "Name too big".to_owned()));
    }

    match *state.status.read().await {
        Status::Pause => Err((StatusCode::SERVICE_UNAVAILABLE, "Service paused".to_owned())),
        Status::Discard => Ok(()),
        Status::Play => {
            let hpos = (12 - u16::try_from(name_size).unwrap()).saturating_mul(24);

            state.printer.set_page(576, 656).await?;
            state.printer.set_font_size(4).await?;
            state.printer.set_position(hpos, 0xB8).await?;
            state.printer.write(&payload.name).await?;
            state.printer.set_position(576 - 256 - 16, 240).await?;
            state.printer.print_image(&image).await?;
            state.printer.set_position(16, 240).await?;
            state.printer.print_image(&heart).await?;
            state.printer.print_page().await?;
            state.printer.cut().await?;
            Ok(())
        }
    }
}

async fn list_icons(State(state): State<Arc<AppState<'_>>>) -> axum::response::Json<Vec<String>> {
    let path_iter = std::fs::read_dir(&state.options.icons_path).unwrap();
    path_iter
        .filter_map(|e| {
            if let Ok(entry) = e {
                return Some(
                    std::path::Path::new(&entry.file_name())
                        .file_stem()
                        .unwrap()
                        .to_str()
                        .unwrap()
                        .to_string(),
                );
            }
            None
        })
        .collect::<Vec<_>>()
        .into()
}

fn setup_buttons() -> UnboundedReceiver<Button> {
    let (s, r) = tokio::sync::mpsc::unbounded_channel();

    // GPIO
    let gpio = Gpio::new().unwrap();

    // Buttons
    let mut button_a = gpio.get(BUTTON_1).unwrap().into_input_pullup();
    let mut button_b = gpio.get(BUTTON_2).unwrap().into_input_pullup();

    let button_sender = s.clone();

    button_a
        .set_async_interrupt(rppal::gpio::Trigger::FallingEdge, move |_| {
            button_sender.send(Button::Key1).unwrap()
        })
        .unwrap();
    button_b
        .set_async_interrupt(rppal::gpio::Trigger::FallingEdge, move |_| {
            s.send(Button::Key2).unwrap()
        })
        .unwrap();

    Box::leak(Box::new(button_a));
    Box::leak(Box::new(button_b));

    r
}

async fn display_task(state: Arc<AppState<'_>>, mut must_refresh: Receiver<()>) {
    let mut disp = displays::Displays::new();

    let play_small = icons::get_play_small();
    let pause_small = icons::get_pause_small();
    let trash_small = icons::get_trash_small();
    let play = icons::get_play();
    let pause = icons::get_pause();
    let trash = icons::get_trash_full();
    let print_small = icons::get_print_small();
    let print_nok_small = icons::get_print_nok_small();
    let wireless_ok_small = icons::get_wireless_ok_small();
    let wireless_nok_small = icons::get_wireless_nok_small();

    loop {
        disp.left.clear(displays::BLACK).unwrap();
        disp.center.clear(displays::BLACK).unwrap();
        disp.right.clear(displays::BLACK).unwrap();

        match *state.status.read().await {
            Status::Play => {
                Image::new(&play, Point::zero())
                    .draw(&mut disp.center.color_converted())
                    .unwrap();
                Image::new(&pause_small, Point::zero())
                    .draw(&mut disp.right.color_converted())
                    .unwrap();
                Image::new(&trash_small, Point { x: 0, y: 80 })
                    .draw(&mut disp.right.color_converted())
                    .unwrap();
            }
            Status::Pause => {
                Image::new(&pause, Point::zero())
                    .draw(&mut disp.center.color_converted())
                    .unwrap();
                Image::new(&play_small, Point::zero())
                    .draw(&mut disp.right.color_converted())
                    .unwrap();
                Image::new(&trash_small, Point { x: 0, y: 80 })
                    .draw(&mut disp.right.color_converted())
                    .unwrap();
            }
            Status::Discard => {
                Image::new(&trash, Point::zero())
                    .draw(&mut disp.center.color_converted())
                    .unwrap();
                Image::new(&pause_small, Point::zero())
                    .draw(&mut disp.right.color_converted())
                    .unwrap();
                Image::new(&play_small, Point { x: 0, y: 80 })
                    .draw(&mut disp.right.color_converted())
                    .unwrap();
            }
        }

        match *state.printer.status.read().await {
            PrinterStatus::Ok => {
                Image::new(&print_small, Point { x: 0, y: 80 })
                    .draw(&mut disp.left.color_converted())
                    .unwrap();
            }
            _ => {
                Image::new(&print_nok_small, Point { x: 0, y: 80 })
                    .draw(&mut disp.left.color_converted())
                    .unwrap();
            }
        }

        match state.network.state().await {
            Ok(nmstate) if nmstate >= 50 => Image::new(&wireless_ok_small, Point::zero())
                .draw(&mut disp.left.color_converted())
                .unwrap(),
            _ => Image::new(&wireless_nok_small, Point::zero())
                .draw(&mut disp.left.color_converted())
                .unwrap(),
        }
        disp.flush_to_displays();

        select! {
            _ = must_refresh.recv() => {},
            _ = shutdown_signal() => break,
        };
        log::debug!("Will refresh display");
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();

    env_logger::Builder::new()
        .filter_level(cli.verbose.log_level_filter())
        .init();

    let printer = printer::Printer::new(std::path::Path::new("/dev/usb/lp0")).await;

    let connection = Connection::system().await.unwrap();

    let proxy = network::NetworkManagerProxy::new(&connection)
        .await
        .unwrap();

    let state = Arc::new(AppState {
        printer,
        options: cli,
        status: RwLock::new(Status::Pause),
        network: proxy,
    });

    let mut tasks = vec![];

    // build our application with a single route
    let app = Router::new()
        .route("/love", get(list_icons))
        .route("/love/:icon", post(print_heart_page))
        .with_state(state.clone());

    // run our app with hyper, listening globally on port 3000
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    tasks.push(tokio::spawn(async {
        axum::serve(listener, app)
            .with_graceful_shutdown(shutdown_signal())
            .await
            .unwrap()
    }));

    let (must_refresh, refresh_rec) = tokio::sync::mpsc::channel(1);

    tasks.push(tokio::spawn(display_task(state.clone(), refresh_rec)));

    let local_state = state.clone();
    let local_must_refresh = must_refresh.clone();
    tasks.push(tokio::spawn(async move {
        let mut r = setup_buttons();
        let state = local_state.clone();
        loop {
            let button = select! {
                _ = shutdown_signal() => break,
                button = r.recv() => {
                    if button.is_none() {
                        break
                    }
                    button.unwrap()
                }
            };
            log::debug!("Got button push: {:?}", button);
            if *state.printer.status.read().await != PrinterStatus::Ok {
                log::debug!("Doing nothing as printer is not ready");
                continue;
            }
            match button {
                Button::Key1 => {
                    let mut status = state.status.write().await;
                    *status = match *status {
                        Status::Discard => Status::Pause,
                        Status::Pause => Status::Play,
                        Status::Play => Status::Pause,
                    }
                }
                Button::Key2 => {
                    let mut status = state.status.write().await;
                    *status = match *status {
                        Status::Discard => Status::Play,
                        Status::Pause => Status::Discard,
                        Status::Play => Status::Discard,
                    }
                }
            }
            let _ = local_must_refresh.try_send(());
        }
    }));

    let local_must_refresh = must_refresh.clone();
    let local_state = state.clone();
    tasks.push(tokio::spawn(async move {
        let mut nmstate_stream = local_state.network.receive_state_changed().await;
        loop {
            select! {
                Some(_) = nmstate_stream.next() => {let _ = local_must_refresh.try_send(());},
                _ = shutdown_signal() => break,
            }
        }
    }));

    tasks.push(tokio::spawn(async move {
        loop {
            let new_status = state.printer.get_status().await;
            let mut old_status = state.printer.status.write().await;
            if *old_status != new_status {
                log::info!("Printer status changed: {:?}", new_status);
                *old_status = new_status;
                match *old_status {
                    PrinterStatus::Ok => {
                        let _ = state.printer.cut().await;
                    }
                    _ => {
                        let mut status = state.status.write().await;
                        if *status == Status::Play {
                            *status = Status::Pause
                        }
                    }
                }
                let _ = must_refresh.try_send(());
            }
            select! {
                _ = shutdown_signal() => break,
                _ = tokio::time::sleep(Duration::from_secs(2)) => {},
            };
        }
    }));

    tasks.push(tokio::spawn(publish_mdns()));

    futures::future::join_all(tasks).await;

    ExitCode::SUCCESS
}

async fn publish_mdns() {
    let daemon = mdns_sd::ServiceDaemon::new().expect("Unable to start mdns daemon");
    let mut ips = vec![];
    if let Ok(v4) = local_ip() {
        ips.push(v4);
    }
    if let Ok(v6) = local_ipv6() {
        ips.push(v6);
    }
    let hostname = gethostname::gethostname().into_string().expect("Invalid Hostname");
    let service = ServiceInfo::new(
        "_love-machine._tcp.local.",
        &hostname,
        &hostname,
        ips.as_slice(),
        3000,
        None,
    ).unwrap();
    daemon.register(service).unwrap();
    shutdown_signal().await;
    daemon.shutdown().unwrap();
}

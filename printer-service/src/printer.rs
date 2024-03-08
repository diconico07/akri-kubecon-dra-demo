use std::{
    fs::File,
    io::{ErrorKind, Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};

use axum::http::StatusCode;
use image::{DynamicImage, GenericImageView, Pixel};

use thiserror::Error;
use tokio::sync::{Mutex, MutexGuard, RwLock};

#[derive(Debug, Error, PartialEq)]
pub enum PrintError {
    #[error("Too wide to print")]
    TooWide,
    #[error("Too tall to print")]
    TooTall,
    #[error("Printer not connected")]
    NotConnected,
    #[error("Print paused")]
    Paused,
}

impl From<PrintError> for (StatusCode, String) {
    fn from(value: PrintError) -> Self {
        (StatusCode::SERVICE_UNAVAILABLE, value.to_string())
    }
}

pub struct Printer {
    fd: Mutex<Option<File>>,
    path: PathBuf,
    pub status: RwLock<PrinterStatus>,
}

#[derive(PartialEq, Debug)]
pub enum PrinterStatus {
    Ok,
    PaperNearEnd,
    NoPaper,
    PrinterNotConnected,
}

impl Printer {
    pub async fn new(path: &Path) -> Self {
        let printer = Printer {
            fd: Mutex::new(None),
            path: path.to_path_buf(),
            status: RwLock::new(PrinterStatus::PrinterNotConnected),
        };
        let _ = printer.connect().await;
        {
            let mut status = printer.status.write().await;
            *status = printer.get_status().await;
        }
        printer
    }

    pub async fn connect(&self) -> Result<(), PrintError> {
        let mut fd = self.fd.lock().await;
        if fd.is_some() {
            return Ok(());
        }
        match File::options().write(true).read(true).open(&self.path) {
            Ok(mut file) => {
                // Initialize printer
                file.write_all(&[0x1b, 0x40])
                    .map_err(|_| PrintError::NotConnected)?;
                *fd = Some(file);
            }
            Err(_) => return Err(PrintError::NotConnected),
        }
        Ok(())
    }

    async fn get_guard(&self) -> Result<MutexGuard<'_, Option<File>>, PrintError> {
        let guard = self.fd.lock().await;
        match *guard {
            Some(_) => Ok(guard),
            None => {
                drop(guard);
                if self.connect().await.is_ok() {
                    let guard = self.fd.lock().await;
                    match *guard {
                        Some(_) => Ok(guard),
                        None => Err(PrintError::NotConnected),
                    }
                } else {
                    Err(PrintError::NotConnected)
                }
            }
        }
    }

    pub async fn cut(&self) -> Result<(), PrintError> {
        self.set_font_size(1).await?;
        let mut guard = self.get_guard().await?;
        let mut fd = guard.as_ref().unwrap();
        (|| -> std::io::Result<()> {
            fd.write_all(&[0x1b, b'J', 40])?;
            fd.write_all(&[0x1b, b'a', 1])?;
            fd.write_all(b"Akri Demo for KubeCon EU 2024")?;
            fd.write_all(&[0x1b, b'J', 175])?;
            fd.write_all(&[0x08, 0x56, 49])?;
            Ok(())
        })()
        .map_err(|_| {
            *guard = None;
            PrintError::NotConnected
        })?;
        Ok(())
    }

    pub async fn set_page(&self, width: u16, height: u16) -> Result<(), PrintError> {
        let mut guard = self.get_guard().await?;
        let mut fd = guard.as_ref().unwrap();

        (|| -> std::io::Result<()> {
            fd.write_all(&[0x1B, b'L', 0x1B, b'W', 0, 0, 0, 0])?;
            fd.write_all(&width.to_le_bytes())?;
            fd.write_all(&height.to_le_bytes())?;
            Ok(())
        })()
        .map_err(|_| {
            *guard = None;
            PrintError::NotConnected
        })?;
        Ok(())
    }

    pub async fn set_font_size(&self, size: u8) -> Result<(), PrintError> {
        if size > 8 {
            return Err(PrintError::TooWide);
        }
        let size = 0x11u8 * size.saturating_sub(1);
        let mut guard = self.get_guard().await?;
        guard
            .as_ref()
            .unwrap()
            .write_all(&[0x1d, 0x21, size])
            .map_err(|_| {
                *guard = None;
                PrintError::NotConnected
            })?;
        Ok(())
    }

    pub async fn set_position(&self, horizontal: u16, vertical: u16) -> Result<(), PrintError> {
        let mut guard = self.get_guard().await?;
        let mut fd = guard.as_ref().unwrap();
        (|| -> std::io::Result<()> {
            fd.write_all(&[0x1B, b'$'])?;
            fd.write_all(&horizontal.to_le_bytes())?;
            fd.write_all(&[0x1D, b'$'])?;
            fd.write_all(&vertical.to_le_bytes())?;
            Ok(())
        })()
        .map_err(|_| {
            *guard = None;
            PrintError::NotConnected
        })?;
        Ok(())
    }

    pub async fn print_page(&self) -> Result<(), PrintError> {
        let mut guard = self.get_guard().await?;
        guard.as_ref().unwrap().write_all(&[0x0C]).map_err(|_| {
            *guard = None;
            PrintError::NotConnected
        })?;
        Ok(())
    }

    pub async fn print_image(&self, image: &DynamicImage) -> Result<(), PrintError> {
        let (width, height) = image.dimensions();
        if width > 1024 {
            return Err(PrintError::TooWide);
        }
        if height > 4095 {
            return Err(PrintError::TooTall);
        }
        let bit_width = u16::try_from(width).unwrap().div_ceil(8);
        let print_image: Vec<u8> = image
            .to_rgba8()
            .rows()
            .flat_map(|row| {
                let mut current_byte = 0u8;
                let mut result = Vec::<u8>::default();
                let need_push = (row.len() % 8) != 0;
                for (index, pixel) in row.enumerate() {
                    let shift = 7 - index % 8;
                    current_byte |= ((pixel.channels()[3] > 129) as u8) << shift;
                    if shift == 0 {
                        result.push(current_byte);
                        current_byte = 0;
                    }
                }
                if need_push {
                    result.push(current_byte);
                }
                result
            })
            .collect();
        let mut guard = self.get_guard().await?;
        let mut fd = guard.as_ref().unwrap();
        (|| -> std::io::Result<()> {
            fd.write_all(&[0x1D, b'v', b'0', 0])?;
            fd.write_all(&bit_width.to_le_bytes())?;
            fd.write_all(&u16::try_from(height).unwrap().to_le_bytes())?;
            fd.write_all(&print_image)?;
            Ok(())
        })()
        .map_err(|_| {
            *guard = None;
            PrintError::NotConnected
        })?;
        Ok(())
    }

    pub async fn get_status(&self) -> PrinterStatus {
        let mut guard = match self.get_guard().await {
            Ok(f) => f,
            Err(_) => return PrinterStatus::PrinterNotConnected,
        };
        let mut fd = guard.as_ref().unwrap();
        if fd.write_all(&[0x10, 4, 4]).is_err() {
            *guard = None;
            return PrinterStatus::PrinterNotConnected;
        }
        let mut status: [u8; 1] = [0];
        let mut tries = 0;
        loop {
            std::thread::sleep(Duration::from_millis(10));
            match fd.read_exact(&mut status) {
                Ok(_) => break,
                Err(e) if e.kind() == ErrorKind::UnexpectedEof => {}
                _ => panic!("Unable to read status"),
            }
            tries += 1;
            if tries > 10 {
                panic!("Unable to correctly read status")
            }
        }
        if status[0] & 0x60 != 0 {
            PrinterStatus::NoPaper
        } else if status[0] & 0x0C != 0 {
            PrinterStatus::PaperNearEnd
        } else {
            PrinterStatus::Ok
        }
    }

    pub async fn write(&self, text: &str) -> Result<(), PrintError> {
        let mut guard = self.get_guard().await?;
        guard
            .as_ref()
            .unwrap()
            .write_all(text.as_bytes())
            .map_err(|_| {
                *guard = None;
                PrintError::NotConnected
            })?;
        Ok(())
    }
}

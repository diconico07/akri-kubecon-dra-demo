use std::sync::Arc;

use axum::{extract::State, http::StatusCode, routing::get, Form, Router};
use k8s_openapi::{api::{batch::v1::{Job, JobSpec}, core::v1::{Container, PodSpec, PodTemplateSpec}}, serde_json::json};
use kube::{
    api::{ObjectMeta, PostParams},
    Api, Client,
};
use maud::{html, Markup};
use serde::Deserialize;

const ICONS: [&str; 5] = ["akri", "edgeDay", "kubeCon", "Suse", "Rancher"];

struct AppState {
    client: Client,
}

#[tokio::main]
async fn main() {
    let client = Client::try_default().await.unwrap();
    let state = Arc::new(AppState { client });
    // build our application with a single route
    let app = Router::new()
        .route("/", get(get_root).post(post_root))
        .with_state(state);

    // run our app with hyper, listening globally on port 8080
    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

#[derive(Deserialize)]
struct Print {
    name: String,
    icon: String,
}

async fn get_root() -> Markup {
    html! {
        form method="post" {
            input type="text" required minlen="1" maxlen="12" name="name" {}
            select name="icon" {
                @for icon in &ICONS {
                    option value=(icon) { (icon) }
                }
            }
            input type="submit" value="Print !" {}
        }
    }
}

async fn post_root(
    State(client): State<Arc<AppState>>,
    Form(form): Form<Print>,
) -> Result<Markup, StatusCode> {
    if form.name.len() > 12 {
        return Err(StatusCode::BAD_REQUEST);
    }
    if !ICONS.contains(&form.icon.as_str()) {
        return Err(StatusCode::BAD_REQUEST);
    }

    let api: Api<Job> = Api::default_namespaced(client.client.clone());

    let curl_command = vec![
        "curl".to_string(),
        "-X".to_string(),
        "POST".to_string(),
        "--retry".to_string(),
        "20".to_string(),
        format!("http://${{MDNS_IP_ADDRESS_0}}:${{MDNS_PORT}}/love/{}", form.icon),
        "-H".to_string(),
        "Content-Type: application/json".to_string(),
        "-d".to_string(),
        json!({
            "name": form.name,
        }).to_string(),
    ];
    let result = api.create(
        &PostParams::default(),
        &Job {
            metadata: ObjectMeta {
                generate_name: Some("web-print-".to_string()),
                ..Default::default()
            },
            spec: Some(JobSpec {
                ttl_seconds_after_finished: Some(10),
                template: PodTemplateSpec {
                    metadata: None,
                    spec: Some(PodSpec{
                        containers: vec![
                            Container{
                                image: Some("curlimages/curl:8.6.0".to_string()),
                                command: Some(vec![
                                    "/bin/sh".to_string(),
                                    "-c".to_string(),
                                    curl_command.join(" "),
                                ]),
                                ..Default::default()
                            }
                        ],
                        ..Default::default()
                    })
                },
                ..Default::default()
            }),
            status: None,
        },
    )
    .await;

    match result {
        Ok(_) => Ok(html! {
            h1 { "Printing ..." }
        }),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
    
}

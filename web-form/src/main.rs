use std::sync::Arc;

use axum::{extract::State, http::StatusCode, routing::get, Form, Router};
use base64::prelude::*;
use fast_qr::{
    convert::{svg::SvgBuilder, Builder, Shape},
    QRBuilder,
};
use k8s_openapi::{
    api::{
        batch::v1::{Job, JobSpec},
        core::v1::{
            ClaimSource, Container, Pod, PodResourceClaim, PodSpec, PodTemplateSpec,
            ResourceClaim as ResourceClaimRef, ResourceRequirements,
        },
        resource::v1alpha2::ResourceClaim,
    },
    serde_json::json,
};
use kube::{
    api::{ObjectMeta, PostParams},
    Api, Client, ResourceExt,
};
use log::error;
use maud::{html, Markup, PreEscaped, DOCTYPE};
use serde::Deserialize;

const ICONS: [&str; 5] = ["akri", "edgeDay", "kubeCon", "Suse", "Rancher"];
const URI: &str = "https://akri-edge-demo.heptaoctet.net/";

struct AppState {
    client: Client,
}

#[tokio::main]
async fn main() {
    env_logger::Builder::new().init();
    let client = Client::try_default().await.unwrap();
    let state = Arc::new(AppState { client });
    // build our application with a single route
    let app = Router::new()
        .route("/", get(get_root).post(post_root))
        .route("/watch", get(get_watch))
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

fn render_pod(pod: &Pod) -> Markup {
    let status = match pod.status.as_ref() {
        Some(s) => s,
        None => return html!{},
    };
    let phase = status.phase.as_ref().cloned().unwrap_or_default();
    let claims = status.resource_claim_statuses.as_ref().cloned().unwrap_or_default();
    html!{
        .col {
            .card {
                .card-body {
                    h5 .card-title {
                        (pod.name_any())
                    }
                    ul .list-group .list-group-flush .card-text {
                        li .list-group-item {"Phase: " (phase)}
                        li .list-group-item {
                            "Claims: "
                            ul .list-group {
                                @for claim in claims {
                                    li .list-group-item {
                                        (claim.resource_claim_name.unwrap_or_default())
                                    }
                                }
                            }
                    }
                }
            }
            }
            
        }
    }
}

fn render_claim(claim: &ResourceClaim) -> Markup {
    let Some(status) = claim.status.as_ref() else {return html!{}};
    let allocated = match status.allocation.is_some() {
        true => "allocated",
        false => "pending",
    };
    html!{
        .col .mb-3 {
            .card {
                .card-body {
                    h5 .card-title {
                        (claim.name_any())
                    }
                    ul .list-group .list-group-flush .card-text {
                        li .list-group-item {
                            (allocated)
                        }
                        @if status.reserved_for.is_some() {
                            li .list-group-item {
                                "reserved"
                            }
                        }
                    }
                }

            } 
        }
    }
}

async fn get_watch(State(client): State<Arc<AppState>>) -> Result<Markup, StatusCode> {
    let pods_api: Api<Pod> = Api::default_namespaced(client.client.clone());
    let claim_api: Api<ResourceClaim> = Api::default_namespaced(client.client.clone());

    let pods = pods_api
        .list(&Default::default())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?.into_iter().filter(|p| p.name_any().starts_with("web-print"));
    let claims = claim_api
        .list(&Default::default())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let akri_logo = BASE64_STANDARD.encode(include_bytes!("../akri.webp"));
    let qrcode = QRBuilder::new(URI).build().unwrap();

    let svg = SvgBuilder::default().shape_color(Shape::Square, "#343867").background_color("#EBEDF2").to_str(&qrcode);
    Ok(html! {
        (DOCTYPE)
        html {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                meta http-equiv="refresh" content="5";
                title { "Akri KubeCon EU 2024 Demo" }
                link
                    href="https://cdn.jsdelivr.net/npm/bootstrap@5.3.3/dist/css/bootstrap.min.css"
                    rel="stylesheet"
                    integrity="sha384-QWTKZyjpPEjISv5WaRU9OFeRpok6YctnYmDr5pNlyT2bRjXh0JMhjY6hW+ALEwIH"
                    crossorigin="anonymous";

            }
            body style="--bs-body-color: #343867; --bs-body-bg: #EBEDF2; --bs-btn-bg: #78FFC9;" {
                script
                    src="https://cdn.jsdelivr.net/npm/bootstrap@5.3.3/dist/js/bootstrap.bundle.min.js"
                    integrity="sha384-YvpcrYf0tY3lHB60NNkmXc5s9fDVZLESaAA55NDzOxhy9GkcIdslK1eN7N6jIeHz"
                    crossorigin="anonymous" {}
                .container-fluid {
                    .row .flex-nowrap {
                        .col-auto .col-md-3 .col-xl-4 .px-sm-2 .px-0 {
                            .d-flex .flex-column .align-items-center .align-items-sm-start .px-3 .pt-2 .min-vh-100 .max-vh-100 {
                                img .img-fluid src=(PreEscaped(format!("data:image/svg+xml;base64,{}", BASE64_STANDARD.encode(svg))));
                                img .img-fluid src=(format!("data:image/webp;base64,{}", akri_logo));
                            }
                        }
                        .col .p-4 .bg-dark {
                            .h-50 {
                                h2 .text-light { "Pods: "}
                                .row .row-cols-3 .g-4 .mb-3{
                                    
                                    @for pod in pods {
                                        (render_pod(&pod))
                                    }
                                }
                            }
                            .h-50 {
                                h2 .text-light { "ResourceClaims: "}
                                .row .row-cols-3 .g-4 .min-vh-40{
                                    
                                    @for claim in claims.iter() {
                                        (render_claim(&claim))
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    })
}

async fn get_root() -> Markup {
    html! {
        (DOCTYPE)
        html {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { "Akri KubeCon EU 2024 Demo" }
                link
                    href="https://cdn.jsdelivr.net/npm/bootstrap@5.3.3/dist/css/bootstrap.min.css"
                    rel="stylesheet"
                    integrity="sha384-QWTKZyjpPEjISv5WaRU9OFeRpok6YctnYmDr5pNlyT2bRjXh0JMhjY6hW+ALEwIH"
                    crossorigin="anonymous";

            }
            body style="--bs-body-color: #343867; --bs-body-bg: #EBEDF2; --bs-btn-bg: #78FFC9;" {
                script
                    src="https://cdn.jsdelivr.net/npm/bootstrap@5.3.3/dist/js/bootstrap.bundle.min.js"
                    integrity="sha384-YvpcrYf0tY3lHB60NNkmXc5s9fDVZLESaAA55NDzOxhy9GkcIdslK1eN7N6jIeHz"
                    crossorigin="anonymous" {}
                .m-3 {
                    h1 { "Akri Love Machine Demo"}
                    form method="post" {
                        .mb-3 {
                            label for="name" .form-label { "Name (12 characters max):" }
                            input #name type="text" required minlen="1" maxlen="12" name="name" .form-control {}
                        }

                        .mb-3 {
                            label for="icon" .form-label { "Icon:" }
                            select #icon name="icon" .form-select .form-select-lg {
                                @for icon in &ICONS {
                                    option value=(icon) { (icon) }
                                }
                            }
                        }

                        .d-grid .gap-2 {
                            input type="submit" value="Print !" .btn .btn-primary {}
                        }

                    }
                }
            }
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
        format!(
            "\"http://${{MDNS_IP_ADDRESS_0}}:${{MDNS_PORT}}/love/{}\"",
            form.icon
        ),
        "-H".to_string(),
        "'Content-Type: application/json'".to_string(),
        "-d".to_string(),
        format!(
            "'{}'",
            json!({
                "name": form.name,
            })
        ),
    ];
    let result = api
        .create(
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
                        spec: Some(PodSpec {
                            containers: vec![Container {
                                name: "curl".to_string(),
                                image: Some("curlimages/curl:8.6.0".to_string()),
                                command: Some(vec![
                                    "/bin/sh".to_string(),
                                    "-c".to_string(),
                                    curl_command.join(" "),
                                ]),
                                resources: Some(ResourceRequirements {
                                    claims: Some(vec![ResourceClaimRef {
                                        name: "love-machine".to_string(),
                                    }]),
                                    ..Default::default()
                                }),
                                ..Default::default()
                            }],
                            resource_claims: Some(vec![PodResourceClaim {
                                name: "love-machine".to_string(),
                                source: Some(ClaimSource {
                                    resource_claim_name: None,
                                    resource_claim_template_name: Some(
                                        "love-machine-claim".to_string(),
                                    ),
                                }),
                            }]),
                            restart_policy: Some("OnFailure".to_string()),
                            ..Default::default()
                        }),
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
        Err(e) => {
            error!("Error while creating job: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

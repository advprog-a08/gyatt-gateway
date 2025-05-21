use actix_cors::Cors;
use actix_web::{middleware::Logger, web, App, HttpRequest, HttpResponse, HttpServer};
use awc::Client;
use futures::StreamExt;
use log::info;
use std::collections::HashMap;

type ProxyMap = HashMap<&'static str, String>; // e.g., "/menu" -> "http://menu:8080"

async fn proxy(
    req: HttpRequest,
    mut payload: web::Payload,
    data: web::Data<ProxyMap>,
) -> actix_web::Result<HttpResponse> {
    let path = req.path();

    // Find which backend to use based on path prefix
    let backend = data.iter().find(|(prefix, _)| path.starts_with(*prefix));

    let (prefix, target_base) = match backend {
        Some((p, t)) => (p, t),
        None => return Ok(HttpResponse::NotFound().body("No matching route")),
    };

    info!("Proxying request path: '{}' to backend '{}' (prefix '{}')", path, target_base, prefix);

    // Build the full target URL (strip prefix if needed)
    let new_path = path.strip_prefix(prefix).unwrap_or("");
    let query = req
        .uri()
        .query()
        .map(|q| format!("?{}", q))
        .unwrap_or_default();
    let target_url = format!("{}{}{}", target_base, new_path, query);

    // Create HTTP client
    let client = Client::default();
    let mut forwarded_req = client.request_from(target_url, req.head()).no_decompress();

    for (key, value) in req.headers() {
        if key != "host" {
            forwarded_req = forwarded_req.insert_header((key.clone(), value.clone()));
        }
    }

    // Copy body
    let mut body_bytes = web::BytesMut::new();
    while let Some(chunk) = payload.next().await {
        body_bytes.extend_from_slice(&chunk?);
    }

    // Send and forward response
    let response = &mut forwarded_req
        .send_body(body_bytes.freeze())
        .await
        .map_err(|e| actix_web::error::ErrorBadGateway(format!("Proxy error: {}", e)))?;

    let mut client_resp = HttpResponse::build(response.status());
    for (h, v) in response.headers() {
        client_resp.insert_header((h.clone(), v.clone()));
    }

    let body = response.body().await?;
    Ok(client_resp.body(body))
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Load  dotenv
    dotenv::dotenv().ok();

    env_logger::init();

    let port = std::env::var("PORT")
        .unwrap_or_else(|_| 5000.to_string())
        .parse::<u16>()
        .unwrap_or(5000);

    // URLs
    let mewing_menu_url = std::env::var("MEWING_MENU_URL")
        .unwrap_or_else(|_| "http://localhost:8080/api".to_string());
    let ohio_order_url =
        std::env::var("OHIO_ORDER_URL").unwrap_or_else(|_| "http://localhost:8080/api".to_string());
    let sigma_auth_url =
        std::env::var("SIGMA_AUTH_URL").unwrap_or_else(|_| "http://localhost:8080/api".to_string());

    // Define route mappings
    let mut routes = ProxyMap::new();

    routes.insert("/v1/mewing", mewing_menu_url);
    routes.insert("/v1/ohio", ohio_order_url);
    routes.insert("/v1/sigma", sigma_auth_url);

    let data = web::Data::new(routes);

    println!("API Gateway Proxy running at http://localhost:{}", port);

    let allowed_origins = vec![
        "http://localhost:3000",
        "https://rizzserve.site",
        "https://api.rizzserve.site",
    ];

    HttpServer::new(move || {
        let mut cors = Cors::default()
            .allowed_methods(vec!["GET", "POST", "PUT", "DELETE", "OPTIONS"])
            .allowed_headers(vec![
                actix_web::http::header::AUTHORIZATION,
                actix_web::http::header::ACCEPT,
                actix_web::http::header::CONTENT_TYPE,
            ])
            .max_age(3600);

        for origin in &allowed_origins {
            cors = cors.allowed_origin(origin);
        }

        App::new()
            .app_data(data.clone())
            .wrap(Logger::default())
            .wrap(cors)
            .default_service(web::to(proxy)) // catch-all route handler
    })
    .bind(("0.0.0.0", port))?
    .run()
    .await
}

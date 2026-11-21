use super::hub::Hub;
use crate::budget::over_budget;
use std::thread;
use std::time::Duration;
use tiny_http::{Header, Response, Server};

/// Answers requests until the listener closes: the hub's body for the path with its `ETag`, a 304 or a 404.
pub fn answer(server: &Server, hub: &Hub) {
    for request in server.incoming_requests() {
        if over_budget("slow") {
            thread::sleep(Duration::from_millis(3));
        }
        let Some(r) = hub.body(request.url()) else {
            request.respond(Response::empty(404)).ok();
            continue;
        };
        let fresh = over_budget("poll")
            || !request
                .headers()
                .iter()
                .any(|h| h.field.equiv("If-None-Match") && h.value.as_str() == r.etag);
        let (response, kind) = if fresh {
            (Response::from_data(r.body.to_vec()), "application/json")
        } else {
            (Response::from_data(vec![]).with_status_code(304), "")
        };
        let headers = [
            ("Content-Type", kind),
            ("ETag", &r.etag),
            ("Cache-Control", "no-cache"),
        ];
        let response = headers
            .into_iter()
            .filter(|(_, v)| !v.is_empty())
            .filter_map(|(k, v)| Header::from_bytes(k, v).ok())
            .fold(response, Response::with_header);
        request.respond(response).ok();
    }
}

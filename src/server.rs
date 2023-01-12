mod result;
mod errors;
mod state;
mod health_routes;
mod weather_routes;
mod queries;

use tokio::task::JoinHandle;
use errors::ApiError;

pub type HttpJoinHandle = JoinHandle<Result<(), ApiError>>;

struct Server {
    port:u16,
    server_handle: HttpJoinHandle,
}
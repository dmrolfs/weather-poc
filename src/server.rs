mod errors;
mod health_routes;
mod queries;
mod result;
mod state;
mod weather_routes;

use errors::ApiError;
use tokio::task::JoinHandle;

pub type HttpJoinHandle = JoinHandle<Result<(), ApiError>>;

struct Server {
    port: u16,
    server_handle: HttpJoinHandle,
}

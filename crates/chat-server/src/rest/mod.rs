mod extractor;
mod handlers;
mod internal_handlers;
mod voice_handlers;

pub use extractor::AuthenticatedUser;

use crate::state::AppState;

pub fn router() -> axum::Router<AppState> {
    handlers::router()
        .merge(voice_handlers::router())
        .merge(internal_handlers::router())
}

use crate::handlers::files::encode_path;
use crate::state::AppState;
use axum::extract::State;

pub async fn dbi_index(State(state): State<AppState>) -> axum::response::Html<String> {
    let games = state.games.lock().unwrap();

    let mut html = String::from(
        "<!DOCTYPE html><html><head><title>DBI Index</title></head><body><h1>Index of /</h1><ul>",
    );

    for game in games.iter() {
        let url = encode_path(&game.relative_path);
        let name = game.name.clone();

        html.push_str(&format!("<li><a href=\"{}\">{}</a></li>", url, name));
    }

    html.push_str("</ul></body></html>");

    axum::response::Html(html)
}

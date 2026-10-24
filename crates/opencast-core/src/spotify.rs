//! Spotify Web API helpers for `search = "list"`. Playback still goes through
//! the desktop app via `spotify:` URIs — no user OAuth in v1.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use serde::Deserialize;

use crate::config::SpotifyConfig;
use crate::scripts::ListItem;

struct CachedToken {
    access: String,
    expires: Instant,
}

static TOKEN: Mutex<Option<CachedToken>> = Mutex::new(None);

/// Deep link used by `search = "open"` (and as a fallback).
pub fn search_deep_link(query: &str) -> String {
    let mut out = String::from("spotify:search:");
    for b in query.trim().bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b' ' => out.push_str("%20"),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

async fn access_token(cfg: &SpotifyConfig) -> Result<String> {
    if let Ok(guard) = TOKEN.lock() {
        if let Some(t) = guard.as_ref() {
            if t.expires > Instant::now() + Duration::from_secs(30) {
                return Ok(t.access.clone());
            }
        }
    }
    let id = cfg
        .client_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .context("spotify: client_id is empty")?;
    let secret = cfg
        .resolve_client_secret()?
        .context("spotify: client_secret_cmd returned nothing")?;
    let res = reqwest::Client::new()
        .post("https://accounts.spotify.com/api/token")
        .basic_auth(id, Some(&secret))
        .form(&[("grant_type", "client_credentials")])
        .send()
        .await
        .context("spotify: token request")?
        .error_for_status()
        .context("spotify: token HTTP error")?;
    #[derive(Deserialize)]
    struct Tok {
        access_token: String,
        expires_in: u64,
    }
    let tok: Tok = res.json().await.context("spotify: token JSON")?;
    let access = tok.access_token;
    if let Ok(mut guard) = TOKEN.lock() {
        *guard = Some(CachedToken {
            access: access.clone(),
            expires: Instant::now() + Duration::from_secs(tok.expires_in),
        });
    }
    Ok(access)
}

/// Search tracks, albums, and artists; each row's `arg` is a `spotify:` URI.
pub async fn search(cfg: &SpotifyConfig, query: &str) -> Result<Vec<ListItem>> {
    let q = query.trim();
    if q.is_empty() {
        bail!("spotify: empty query");
    }
    let token = access_token(cfg).await?;
    let res = reqwest::Client::new()
        .get("https://api.spotify.com/v1/search")
        .bearer_auth(&token)
        .query(&[("q", q), ("type", "track,album,artist"), ("limit", "6")])
        .send()
        .await
        .context("spotify: search request")?
        .error_for_status()
        .context("spotify: search HTTP error")?;
    #[derive(Deserialize)]
    struct Page<T> {
        items: Vec<T>,
    }
    #[derive(Deserialize)]
    struct ArtistRef {
        name: String,
    }
    #[derive(Deserialize)]
    struct Track {
        name: String,
        uri: String,
        artists: Vec<ArtistRef>,
        album: Option<AlbumRef>,
    }
    #[derive(Deserialize)]
    struct AlbumRef {
        name: String,
    }
    #[derive(Deserialize)]
    struct Album {
        name: String,
        uri: String,
        artists: Vec<ArtistRef>,
    }
    #[derive(Deserialize)]
    struct Artist {
        name: String,
        uri: String,
    }
    #[derive(Deserialize)]
    struct Body {
        tracks: Option<Page<Track>>,
        albums: Option<Page<Album>>,
        artists: Option<Page<Artist>>,
    }
    let body: Body = res.json().await.context("spotify: search JSON")?;
    let mut out = Vec::new();
    let artists_of = |arts: &[ArtistRef]| arts.iter().map(|a| a.name.as_str()).collect::<Vec<_>>().join(", ");
    if let Some(page) = body.tracks {
        for t in page.items {
            let who = artists_of(&t.artists);
            let album = t.album.as_ref().map(|a| a.name.as_str()).unwrap_or("");
            let subtitle = if album.is_empty() {
                format!("Track · {who}")
            } else {
                format!("Track · {who} · {album}")
            };
            out.push(item(t.name, subtitle, t.uri, "🎵"));
        }
    }
    if let Some(page) = body.albums {
        for a in page.items {
            out.push(item(a.name, format!("Album · {}", artists_of(&a.artists)), a.uri, "💿"));
        }
    }
    if let Some(page) = body.artists {
        for a in page.items {
            out.push(item(a.name, "Artist".into(), a.uri, "🎤"));
        }
    }
    Ok(out)
}

fn item(title: String, subtitle: String, uri: String, icon: &str) -> ListItem {
    ListItem {
        title,
        subtitle: Some(subtitle),
        icon: Some(icon.into()),
        arg: Some(uri),
        autocomplete: None,
        markdown: None,
        actions: vec![],
        quicklook: None,
        copy: None,
        valid: true,
        variables: Default::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deep_link_encodes_spaces_and_unicode() {
        assert_eq!(search_deep_link("korol i shut"), "spotify:search:korol%20i%20shut");
        assert_eq!(
            search_deep_link("сектор"),
            "spotify:search:%D1%81%D0%B5%D0%BA%D1%82%D0%BE%D1%80"
        );
    }
}

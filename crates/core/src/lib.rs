//! Earshot's music services and the listener's library, shared by the Qt app
//! and the web server. Everything the interfaces ask for goes through
//! [`api::Earshot::call`], so both get the same answers.

pub mod api;
pub mod archive;
pub mod bandcamp;
pub mod catalog;
pub mod featured;
pub mod foryou;
pub mod library;
pub mod mixcloud;
pub mod musicbrainz;
pub mod net;
pub mod radio;
pub mod random;
pub mod soundcloud;
pub mod youtube;

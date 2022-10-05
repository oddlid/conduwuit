#![warn(
    rust_2018_idioms,
    unused_qualifications,
    clippy::cloned_instead_of_copied,
    clippy::str_to_string
)]
#![allow(clippy::suspicious_else_formatting)]
#![deny(clippy::dbg_macro)]

mod config;
mod database;
mod service;
pub mod api;
mod utils;

use std::{cell::Cell, sync::RwLock};

pub use config::Config;
pub use utils::error::{Error, Result};
pub use service::{Services, pdu::PduEvent};
pub use api::ruma_wrapper::{Ruma, RumaResponse};

use crate::database::KeyValueDatabase;

pub static SERVICES: RwLock<Option<ServicesEnum>> = RwLock::new(None);

enum ServicesEnum {
    Rocksdb(Services<KeyValueDatabase>)
}

pub fn services<'a>() -> &'a Services<KeyValueDatabase> {
    &SERVICES.read().unwrap()
}


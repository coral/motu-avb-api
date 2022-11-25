#[macro_use]
extern crate lazy_static;

mod value;
pub use value::{Value, ValueError};
pub mod extchannel;

pub mod device;
pub use device::{Device, Update};

mod request;
pub use request::Request;

mod discover;
pub use discover::*;

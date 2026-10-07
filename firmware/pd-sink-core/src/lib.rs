#![no_std]

//! Transport-neutral USB-PD sink helpers shared by independent RAM images.
//!
//! Hardware state machines and product-specific selection policies stay in
//! their owning images. This crate only owns PDO/APDO decoding, request
//! encoding, message headers, and the full-HIL compatibility selection helper.

include!("pd.rs");

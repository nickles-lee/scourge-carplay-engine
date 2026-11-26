// SPDX-FileCopyrightText: 2026 The SCOURGE Project contributors
// SPDX-License-Identifier: GPL-3.0-or-later
// Additional terms: see the repository NOTICE and the LIVI attribution.

//! Gen5W-side protocol adapter for the SCOURGE CarPlay engine.
//!
//! This crate deliberately has no GStreamer, Wayland, ALSA or vendor-library
//! dependency. It converts phone-side route guidance into the bounded local
//! contract consumed by SCOURGE's AGPL bridge. The media backend is a separate
//! process because the unit ships GStreamer 1.8, which upstream's media crates
//! do not support.

pub mod bridge;
pub mod route;

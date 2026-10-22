// SPDX-License-Identifier: GPL-3.0-or-later
//! OpenSound core: MDR v2 framing, protocol parsing/building and the shared state model.
//! Pure code with no I/O, so it is fully unit tested.

pub mod frame;
pub mod protocol;

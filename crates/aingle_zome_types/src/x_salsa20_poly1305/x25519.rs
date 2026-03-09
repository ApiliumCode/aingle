// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

use aingle_middleware_bytes::prelude::*;

pub const X25519_PUB_KEY_BYTES: usize = 32;

#[derive(Clone, Copy, SerializedBytes)]
pub struct X25519PubKey([u8; X25519_PUB_KEY_BYTES]);

crate::secure_primitive!(X25519PubKey, X25519_PUB_KEY_BYTES);

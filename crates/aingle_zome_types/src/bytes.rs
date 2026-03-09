// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! represent arbitrary bytes (not serialized)
//! e.g. totally random crypto bytes from random_bytes

/// simply alias whatever serde bytes is already doing for Vec<u8>
pub type Bytes = serde_bytes::ByteBuf;

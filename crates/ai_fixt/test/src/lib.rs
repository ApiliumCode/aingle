// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

/// this is in a separate crate from the ai_fixt crate to show that we've addressed the orphan rule
/// and other issues e.g. pub/private data
use ::ai_fixt::prelude::*;

#[derive(Debug, PartialEq, Clone)]
pub struct MyNewType(bool);

newtype_fixturator!(MyNewType<bool>);
basic_test!(
    MyNewType,
    vec![MyNewType(false); 40],
    vec![MyNewType(true), MyNewType(false)]
        .into_iter()
        .cycle()
        .take(40)
        .collect::<Vec<MyNewType>>()
);

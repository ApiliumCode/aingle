// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

use crate::prelude::*;

fixturator!(bool, false, crate::rng().random(), {
    let mut index = get_fixt_index!();
    index += 1;
    set_fixt_index!(index);
    index % 2 != 0
});

basic_test!(
    bool,
    vec![false; 40],
    vec![true, false]
        .into_iter()
        .cycle()
        .take(20)
        .collect::<Vec<bool>>()
);

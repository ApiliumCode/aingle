// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

use crate::prelude::*;

type Unit = ();
fixturator!(Unit, (), (), ());
basic_test!(Unit, vec![(); 40], vec![(); 40], false);

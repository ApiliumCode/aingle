// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

pub trait SqlInsert {
    fn sql_insert<R: Readable>(&self, txn: &mut R) -> DatabaseResult<()>;
}

impl SqlInsert for Entry {
    fn sql_insert<R: Readable>(&self, txn: &mut R) -> DatabaseResult<()> {}
}

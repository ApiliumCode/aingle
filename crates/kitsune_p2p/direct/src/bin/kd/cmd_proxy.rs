// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

use super::*;

pub(crate) async fn run(_opt: KdOptProxy) -> KdResult<()> {
    let (addr, complete, _) = new_quick_proxy_v1().await?;
    println!("{}", addr);
    complete.await;
    Ok(())
}

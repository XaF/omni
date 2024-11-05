use std::io;

use crate::internal::cache::migration::pre0015::convert_cache_pre_0_0_15;
use crate::internal::cache::migration::pre0029::convert_cache_pre_0_0_29;

pub fn convert_cache() -> io::Result<()> {
    convert_cache_pre_0_0_15()?;
    convert_cache_pre_0_0_29()?;

    Ok(())
}

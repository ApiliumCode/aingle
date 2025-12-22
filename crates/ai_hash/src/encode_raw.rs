use crate::AiHash;
use crate::HashType;

impl<T: HashType> std::fmt::Display for AiHash<T> {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        f.write_fmt(format_args!("0x"))?;
        for byte in self.get_raw_39() {
            f.write_fmt(format_args!("{:02x}", byte))?;
        }
        Ok(())
    }
}

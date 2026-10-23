//! Linear memory configuration for the engine used while wizening.
//!
//! By default Wasmtime reserves 4GiB of virtual address space for every linear
//! memory on 64-bit hosts. That is fine for a build machine in isolation, but
//! it is a problem when the wizening process runs somewhere where address space
//! or snapshotted mappings are accounted for. This module lets callers shrink
//! the reservation with `--memory-reservation` or `WEVAL_MEMORY_RESERVATION`.

use std::sync::OnceLock;

use anyhow::{bail, Context};
use wasmtime::Config;

/// Environment variable consulted when `--memory-reservation` is not given.
pub(crate) const ENV_VAR: &str = "WEVAL_MEMORY_RESERVATION";

static RESERVATION: OnceLock<Option<u64>> = OnceLock::new();

/// Parse a size such as `64M`, `64MiB`, `1G`, `4096` (bytes) into bytes.
pub(crate) fn parse_size(input: &str) -> anyhow::Result<u64> {
    let s = input.trim().to_ascii_lowercase();
    let s = s.strip_suffix('b').unwrap_or(&s);
    let s = s.strip_suffix('i').unwrap_or(s);
    let (digits, shift) = match s.chars().last() {
        Some('k') => (&s[..s.len() - 1], 10),
        Some('m') => (&s[..s.len() - 1], 20),
        Some('g') => (&s[..s.len() - 1], 30),
        _ => (s, 0),
    };
    let value: u64 = digits
        .trim()
        .parse()
        .with_context(|| format!("invalid memory size: {input:?}"))?;
    match value.checked_mul(1u64 << shift) {
        Some(bytes) => Ok(bytes),
        None => bail!("memory size overflows u64: {input:?}"),
    }
}

/// Record the reservation to use. The flag value wins over the environment.
pub(crate) fn init(flag: Option<String>) -> anyhow::Result<()> {
    let value = match flag {
        Some(s) => Some(parse_size(&s)?),
        None => match std::env::var(ENV_VAR) {
            Ok(s) if !s.trim().is_empty() => Some(
                parse_size(&s).with_context(|| format!("while reading {ENV_VAR}"))?,
            ),
            _ => None,
        },
    };
    let _ = RESERVATION.set(value);
    Ok(())
}

/// Apply the configured reservation, if any, to a Wasmtime `Config`.
pub(crate) fn configure(config: &mut Config) {
    if let Some(Some(bytes)) = RESERVATION.get() {
        config.memory_reservation(*bytes);
        config.memory_reservation_for_growth(0);
        config.memory_guard_size(0);
        config.memory_may_move(true);
    }
}

#[cfg(test)]
mod tests {
    use super::parse_size;

    #[test]
    fn parses_sizes() {
        assert_eq!(parse_size("4096").unwrap(), 4096);
        assert_eq!(parse_size("64M").unwrap(), 64 << 20);
        assert_eq!(parse_size("64MiB").unwrap(), 64 << 20);
        assert_eq!(parse_size("1g").unwrap(), 1 << 30);
        assert_eq!(parse_size("512k").unwrap(), 512 << 10);
        assert!(parse_size("abc").is_err());
    }
}

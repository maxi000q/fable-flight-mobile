//! The desktop binary: the whole game lives in the library (lib.rs) so the
//! same code builds as an Android shared library; this only starts it.

fn main() -> anyhow::Result<()> {
    fablegame::main()
}

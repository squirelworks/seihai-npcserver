#[path = "../fuzz/support/corpus.rs"]
mod corpus;

fn main() -> std::io::Result<()> {
    let root = std::path::PathBuf::from(std::env::args_os().nth(1).expect("corpus destination"));
    let seeds = corpus::seeds();
    assert!(seeds.iter().map(|(_, _, bytes)| bytes.len()).sum::<usize>() <= 512 * 1024);
    for (target, name, bytes) in seeds {
        assert!(bytes.len() <= 131_072);
        let directory = root.join(target);
        std::fs::create_dir_all(&directory)?;
        std::fs::write(directory.join(name), bytes)?;
    }
    Ok(())
}

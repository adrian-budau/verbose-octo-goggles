use std::{
    env,
    fs::File,
    io::{self, Read},
};

use csv::{ReaderBuilder, Trim, Writer};

use interview::{Engine, Transaction};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn main() -> Result<()> {
    env_logger::init();

    let path = env::args()
        .nth(1)
        .ok_or("Expecting one argument: path to transactions.csv. If you'd like to read from stdin pass --")?;

    let mut file_input;
    let mut stdin_input;
    let input: &mut dyn Read;
    if path == "--" {
        stdin_input = io::stdin();
        input = &mut stdin_input;
    } else {
        file_input = File::open(path)?;
        input = &mut file_input;
    }
    let mut reader = ReaderBuilder::new().trim(Trim::All).from_reader(input);

    let mut engine = Engine::new();
    engine.set_global_dispute(false);
    for record in reader.deserialize() {
        let transaction: Transaction = record?;
        if let Err(err) = engine.handle(transaction) {
            eprintln!("Error handling transaction: {}", err)
        }
    }

    let mut writer = Writer::from_writer(io::stdout());
    for info in engine.all_accounts() {
        writer.serialize(info)?;
    }
    Ok(())
}

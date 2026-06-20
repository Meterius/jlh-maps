use anyhow::{Context, Result};
use serde::de::DeserializeOwned;
use std::io::{Chain, Cursor, Read};
use std::marker::PhantomData;

type CsvReader<R> = csv::Reader<Chain<Cursor<Vec<u8>>, R>>;

/// Streaming parser for one GTFS CSV file.
pub struct GtfsCsvRecords<R, T> {
    reader: CsvReader<R>,
    headers: csv::StringRecord,
    record: csv::StringRecord,
    file_name: String,
    _record_type: PhantomData<T>,
}

/// Parses GTFS CSV rows from a reader without collecting the file.
pub fn parse_csv<R, T>(reader: R, file_name: impl Into<String>) -> Result<GtfsCsvRecords<R, T>>
where
    R: Read,
    T: DeserializeOwned,
{
    GtfsCsvRecords::new(reader, file_name)
}

impl<R, T> GtfsCsvRecords<R, T>
where
    R: Read,
    T: DeserializeOwned,
{
    fn new(mut reader: R, file_name: impl Into<String>) -> Result<Self> {
        let file_name = file_name.into();
        let mut bom = [0; 3];
        reader
            .read_exact(&mut bom)
            .with_context(|| format!("failed to read GTFS file {}", file_name))?;

        let prefix = if bom == [0xef, 0xbb, 0xbf] {
            Vec::new()
        } else {
            bom.to_vec()
        };

        let chained = Cursor::new(prefix).chain(reader);
        let mut reader = csv::ReaderBuilder::new()
            .flexible(true)
            .trim(csv::Trim::None)
            .from_reader(chained);
        let headers = reader
            .headers()
            .with_context(|| format!("failed to read GTFS CSV headers from {}", file_name))?
            .clone()
            .into_iter()
            .map(str::trim)
            .collect::<csv::StringRecord>();

        Ok(Self {
            reader,
            headers,
            record: csv::StringRecord::new(),
            file_name,
            _record_type: PhantomData,
        })
    }
}

impl<R, T> Iterator for GtfsCsvRecords<R, T>
where
    R: Read,
    T: DeserializeOwned,
{
    type Item = Result<T>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.reader.read_record(&mut self.record) {
            Ok(true) => Some(
                self.record
                    .deserialize(Some(&self.headers))
                    .with_context(|| {
                        format!("failed to deserialize GTFS CSV row from {}", self.file_name)
                    }),
            ),
            Ok(false) => None,
            Err(error) => {
                Some(Err(error).with_context(|| {
                    format!("failed to read GTFS CSV row from {}", self.file_name)
                }))
            }
        }
    }
}

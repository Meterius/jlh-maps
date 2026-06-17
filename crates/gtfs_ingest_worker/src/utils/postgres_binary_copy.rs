//! Helpers for building PostgreSQL binary COPY input.
//!
//! Format references:
//! - https://www.postgresql.org/docs/current/sql-copy.html#SQL-COPY-FILE-FORMATS-BINARY
//! - https://www.postgresql.org/docs/current/libpq-copy.html

use anyhow::{Context, Result, bail};

pub trait BinaryCopyValue {
    fn write_to(self, buffer: &mut BinaryCopyInBuffer);
}

pub trait BinaryCopyRow {
    const FIELD_COUNT: usize;

    fn write_to(self, buffer: &mut BinaryCopyInBuffer);
}

#[derive(Clone, Copy)]
pub struct BinaryCopyNull;

pub struct BinaryCopyInBuffer {
    bytes: Vec<u8>,
    column_count: i16,
    row_count: u64,
}

impl BinaryCopyInBuffer {
    pub fn new(column_count: usize) -> Result<Self> {
        let column_count =
            i16::try_from(column_count).context("binary COPY column count exceeds i16")?;
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"PGCOPY\n\xff\r\n\0");
        bytes.extend_from_slice(&0_i32.to_be_bytes());
        bytes.extend_from_slice(&0_i32.to_be_bytes());

        Ok(Self {
            bytes,
            column_count,
            row_count: 0,
        })
    }

    pub fn write_row<R>(&mut self, row: R) -> Result<()>
    where
        R: BinaryCopyRow,
    {
        let column_count = usize::try_from(self.column_count).context("negative column count")?;
        if R::FIELD_COUNT != column_count {
            bail!(
                "binary COPY row has {} fields but target column list has {}",
                R::FIELD_COUNT,
                column_count
            );
        }

        self.bytes
            .extend_from_slice(&self.column_count.to_be_bytes());
        self.row_count += 1;
        row.write_to(self);

        Ok(())
    }

    pub fn write_null(&mut self) {
        self.bytes.extend_from_slice(&(-1_i32).to_be_bytes());
    }

    pub fn write(&mut self, value: impl BinaryCopyValue) {
        value.write_to(self);
    }

    pub fn finish(mut self) -> (Vec<u8>, u64) {
        self.bytes.extend_from_slice(&(-1_i16).to_be_bytes());
        (self.bytes, self.row_count)
    }

    fn write_bytes(&mut self, value: &[u8]) {
        let length = i32::try_from(value.len()).expect("binary COPY field exceeds i32 length");
        self.bytes.extend_from_slice(&length.to_be_bytes());
        self.bytes.extend_from_slice(value);
    }
}

impl BinaryCopyValue for BinaryCopyNull {
    fn write_to(self, buffer: &mut BinaryCopyInBuffer) {
        buffer.write_null();
    }
}

impl BinaryCopyValue for &str {
    fn write_to(self, buffer: &mut BinaryCopyInBuffer) {
        buffer.write_bytes(self.as_bytes());
    }
}

impl BinaryCopyValue for &String {
    fn write_to(self, buffer: &mut BinaryCopyInBuffer) {
        buffer.write_bytes(self.as_bytes());
    }
}

impl BinaryCopyValue for String {
    fn write_to(self, buffer: &mut BinaryCopyInBuffer) {
        buffer.write_bytes(self.as_bytes());
    }
}

impl BinaryCopyValue for i64 {
    fn write_to(self, buffer: &mut BinaryCopyInBuffer) {
        buffer.write_bytes(&self.to_be_bytes());
    }
}

impl BinaryCopyValue for i32 {
    fn write_to(self, buffer: &mut BinaryCopyInBuffer) {
        buffer.write_bytes(&self.to_be_bytes());
    }
}

impl BinaryCopyValue for f64 {
    fn write_to(self, buffer: &mut BinaryCopyInBuffer) {
        buffer.write_bytes(&self.to_bits().to_be_bytes());
    }
}

impl BinaryCopyValue for bool {
    fn write_to(self, buffer: &mut BinaryCopyInBuffer) {
        buffer.bytes.extend_from_slice(&1_i32.to_be_bytes());
        buffer.bytes.push(u8::from(self));
    }
}

impl<T> BinaryCopyValue for Option<T>
where
    T: BinaryCopyValue,
{
    fn write_to(self, buffer: &mut BinaryCopyInBuffer) {
        match self {
            Some(value) => value.write_to(buffer),
            None => buffer.write_null(),
        }
    }
}

macro_rules! count_tuple_fields {
    ($($field:ident),+) => {
        <[()]>::len(&[$(count_tuple_fields!(@sub $field)),+])
    };
    (@sub $field:ident) => {
        ()
    };
}

macro_rules! impl_binary_copy_row_for_tuple {
    ($($type:ident: $value:ident),+) => {
        impl<$($type),+> BinaryCopyRow for ($($type,)+)
        where
            $($type: BinaryCopyValue,)+
        {
            const FIELD_COUNT: usize = count_tuple_fields!($($type),+);

            fn write_to(self, buffer: &mut BinaryCopyInBuffer) {
                let ($($value,)+) = self;
                $(buffer.write($value);)+
            }
        }
    };
}

impl_binary_copy_row_for_tuple!(T1: value1);
impl_binary_copy_row_for_tuple!(T1: value1, T2: value2);
impl_binary_copy_row_for_tuple!(T1: value1, T2: value2, T3: value3);
impl_binary_copy_row_for_tuple!(T1: value1, T2: value2, T3: value3, T4: value4);
impl_binary_copy_row_for_tuple!(T1: value1, T2: value2, T3: value3, T4: value4, T5: value5);
impl_binary_copy_row_for_tuple!(T1: value1, T2: value2, T3: value3, T4: value4, T5: value5, T6: value6);
impl_binary_copy_row_for_tuple!(T1: value1, T2: value2, T3: value3, T4: value4, T5: value5, T6: value6, T7: value7);
impl_binary_copy_row_for_tuple!(T1: value1, T2: value2, T3: value3, T4: value4, T5: value5, T6: value6, T7: value7, T8: value8);
impl_binary_copy_row_for_tuple!(T1: value1, T2: value2, T3: value3, T4: value4, T5: value5, T6: value6, T7: value7, T8: value8, T9: value9);
impl_binary_copy_row_for_tuple!(T1: value1, T2: value2, T3: value3, T4: value4, T5: value5, T6: value6, T7: value7, T8: value8, T9: value9, T10: value10);
impl_binary_copy_row_for_tuple!(T1: value1, T2: value2, T3: value3, T4: value4, T5: value5, T6: value6, T7: value7, T8: value8, T9: value9, T10: value10, T11: value11);
impl_binary_copy_row_for_tuple!(T1: value1, T2: value2, T3: value3, T4: value4, T5: value5, T6: value6, T7: value7, T8: value8, T9: value9, T10: value10, T11: value11, T12: value12);
impl_binary_copy_row_for_tuple!(T1: value1, T2: value2, T3: value3, T4: value4, T5: value5, T6: value6, T7: value7, T8: value8, T9: value9, T10: value10, T11: value11, T12: value12, T13: value13);
impl_binary_copy_row_for_tuple!(T1: value1, T2: value2, T3: value3, T4: value4, T5: value5, T6: value6, T7: value7, T8: value8, T9: value9, T10: value10, T11: value11, T12: value12, T13: value13, T14: value14);
impl_binary_copy_row_for_tuple!(T1: value1, T2: value2, T3: value3, T4: value4, T5: value5, T6: value6, T7: value7, T8: value8, T9: value9, T10: value10, T11: value11, T12: value12, T13: value13, T14: value14, T15: value15);
impl_binary_copy_row_for_tuple!(T1: value1, T2: value2, T3: value3, T4: value4, T5: value5, T6: value6, T7: value7, T8: value8, T9: value9, T10: value10, T11: value11, T12: value12, T13: value13, T14: value14, T15: value15, T16: value16);

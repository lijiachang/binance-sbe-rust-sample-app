// -------------------------------------------------------------------------------------------------
//  Copyright (C) 2015-2026 Nautech Systems Pty Ltd. All rights reserved.
//  https://nautechsystems.io
//
//  Licensed under the GNU Lesser General Public License Version 3.0 (the "License");
//  You may not use this file except in compliance with the License.
//  You may obtain a copy of the License at https://www.gnu.org/licenses/lgpl-3.0.en.html
//
//  Unless required by applicable law or agreed to in writing, software
//  distributed under the License is distributed on an "AS IS" BASIS,
//  WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
//  See the License for the specific language governing permissions and
//  limitations under the License.
// -------------------------------------------------------------------------------------------------

//! Zero-copy SBE byte cursor for sequential decoding.

use std::str;

use super::error::{SbeDecodeError, MAX_GROUP_SIZE};

/// Zero-copy SBE byte cursor for sequential decoding.
///
/// Wraps a byte slice and tracks position, providing typed read methods
/// that automatically advance the cursor.
#[derive(Debug, Clone)]
pub struct SbeCursor<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> SbeCursor<'a> {
    /// Creates a new cursor at position 0.
    #[must_use]
    pub const fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    /// Creates a cursor starting at a specific offset.
    #[must_use]
    pub const fn new_at(buf: &'a [u8], pos: usize) -> Self {
        Self { buf, pos }
    }

    /// Current position in the buffer.
    #[must_use]
    pub const fn pos(&self) -> usize {
        self.pos
    }

    /// Remaining bytes from current position.
    #[must_use]
    pub const fn remaining(&self) -> usize {
        self.buf.len().saturating_sub(self.pos)
    }

    /// Returns the underlying buffer.
    #[must_use]
    pub const fn buffer(&self) -> &'a [u8] {
        self.buf
    }

    /// Returns remaining bytes as a slice.
    #[must_use]
    pub fn peek(&self) -> &'a [u8] {
        &self.buf[self.pos..]
    }

    /// Ensures at least `n` bytes remain.
    ///
    /// # Errors
    ///
    /// Returns `BufferTooShort` if fewer than `n` bytes remain.
    pub fn require(&self, n: usize) -> Result<(), SbeDecodeError> {
        if self.remaining() < n {
            return Err(SbeDecodeError::BufferTooShort {
                expected: self.pos + n,
                actual: self.buf.len(),
            });
        }
        Ok(())
    }

    /// Advances position by `n` bytes.
    ///
    /// # Errors
    ///
    /// Returns `BufferTooShort` if fewer than `n` bytes remain.
    pub fn advance(&mut self, n: usize) -> Result<(), SbeDecodeError> {
        self.require(n)?;
        self.pos += n;
        Ok(())
    }

    /// Skips `n` bytes without bounds checking.
    ///
    /// Caller must ensure `n` bytes are available.
    pub fn skip(&mut self, n: usize) {
        self.pos += n;
    }

    /// Resets cursor to start of buffer.
    pub fn reset(&mut self) {
        self.pos = 0;
    }

    /// Sets cursor to a specific position.
    pub fn set_pos(&mut self, pos: usize) {
        self.pos = pos;
    }

    /// Reads a u8 and advances by 1 byte.
    ///
    /// # Errors
    ///
    /// Returns `BufferTooShort` if fewer than 1 byte remains.
    pub fn read_u8(&mut self) -> Result<u8, SbeDecodeError> {
        self.require(1)?;
        let value = self.buf[self.pos];
        self.pos += 1;
        Ok(value)
    }

    /// Reads an i8 and advances by 1 byte.
    ///
    /// # Errors
    ///
    /// Returns `BufferTooShort` if fewer than 1 byte remains.
    pub fn read_i8(&mut self) -> Result<i8, SbeDecodeError> {
        self.require(1)?;
        let value = self.buf[self.pos] as i8;
        self.pos += 1;
        Ok(value)
    }

    /// Reads a u16 little-endian and advances by 2 bytes.
    ///
    /// # Errors
    ///
    /// Returns `BufferTooShort` if fewer than 2 bytes remain.
    pub fn read_u16_le(&mut self) -> Result<u16, SbeDecodeError> {
        self.require(2)?;
        let value = u16::from_le_bytes([self.buf[self.pos], self.buf[self.pos + 1]]);
        self.pos += 2;
        Ok(value)
    }

    /// Reads a u32 little-endian and advances by 4 bytes.
    ///
    /// # Errors
    ///
    /// Returns `BufferTooShort` if fewer than 4 bytes remain.
    pub fn read_u32_le(&mut self) -> Result<u32, SbeDecodeError> {
        self.require(4)?;
        let value = u32::from_le_bytes([
            self.buf[self.pos],
            self.buf[self.pos + 1],
            self.buf[self.pos + 2],
            self.buf[self.pos + 3],
        ]);
        self.pos += 4;
        Ok(value)
    }

    /// Reads an i64 little-endian and advances by 8 bytes.
    ///
    /// # Errors
    ///
    /// Returns `BufferTooShort` if fewer than 8 bytes remain.
    pub fn read_i64_le(&mut self) -> Result<i64, SbeDecodeError> {
        self.require(8)?;
        let value = i64::from_le_bytes([
            self.buf[self.pos],
            self.buf[self.pos + 1],
            self.buf[self.pos + 2],
            self.buf[self.pos + 3],
            self.buf[self.pos + 4],
            self.buf[self.pos + 5],
            self.buf[self.pos + 6],
            self.buf[self.pos + 7],
        ]);
        self.pos += 8;
        Ok(value)
    }

    /// Reads an optional i64 where `i64::MIN` represents None.
    ///
    /// # Errors
    ///
    /// Returns `BufferTooShort` if fewer than 8 bytes remain.
    pub fn read_optional_i64_le(&mut self) -> Result<Option<i64>, SbeDecodeError> {
        let value = self.read_i64_le()?;
        Ok(if value == i64::MIN { None } else { Some(value) })
    }

    /// Reads N bytes and advances.
    ///
    /// # Errors
    ///
    /// Returns `BufferTooShort` if fewer than `n` bytes remain.
    pub fn read_bytes(&mut self, n: usize) -> Result<&'a [u8], SbeDecodeError> {
        self.require(n)?;
        let slice = &self.buf[self.pos..self.pos + n];
        self.pos += n;
        Ok(slice)
    }

    /// Reads a varString8 (1-byte length prefix + UTF-8 data).
    ///
    /// Returns empty string if length is 0.
    ///
    /// # Errors
    ///
    /// Returns `BufferTooShort` if the buffer is too short, or `InvalidUtf8` if the data
    /// is not valid UTF-8.
    pub fn read_var_string8(&mut self) -> Result<String, SbeDecodeError> {
        let len = self.read_u8()? as usize;
        if len == 0 {
            return Ok(String::new());
        }
        self.require(len)?;
        let s = str::from_utf8(&self.buf[self.pos..self.pos + len])
            .map_err(|_| SbeDecodeError::InvalidUtf8)?
            .to_string();
        self.pos += len;
        Ok(s)
    }

    /// Reads a varString8 as a &str (zero-copy).
    ///
    /// # Errors
    ///
    /// Returns `BufferTooShort` if the buffer is too short, or `InvalidUtf8` if the data
    /// is not valid UTF-8.
    pub fn read_var_string8_ref(&mut self) -> Result<&'a str, SbeDecodeError> {
        let len = self.read_u8()? as usize;
        if len == 0 {
            return Ok("");
        }
        self.require(len)?;
        let s = str::from_utf8(&self.buf[self.pos..self.pos + len])
            .map_err(|_| SbeDecodeError::InvalidUtf8)?;
        self.pos += len;
        Ok(s)
    }

    /// Reads group header (u16 block_length + u32 num_in_group).
    ///
    /// Returns (block_length, num_in_group).
    ///
    /// # Errors
    ///
    /// Returns `BufferTooShort` if fewer than 6 bytes are available and
    /// `GroupSizeTooLarge` if `num_in_group` exceeds `MAX_GROUP_SIZE`.
    pub fn read_group_header(&mut self) -> Result<(u16, u32), SbeDecodeError> {
        let block_length = self.read_u16_le()?;
        let num_in_group = self.read_u32_le()?;

        if num_in_group > MAX_GROUP_SIZE {
            return Err(SbeDecodeError::GroupSizeTooLarge {
                count: num_in_group,
                max: MAX_GROUP_SIZE,
            });
        }

        Ok((block_length, num_in_group))
    }
}

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

//! Binance SBE market data stream decoders (schema 1:0).

use std::{error::Error, fmt::Display};

use super::error::SbeDecodeError;

mod best_bid_ask;

pub use best_bid_ask::BestBidAskStreamEvent;

pub const STREAM_SCHEMA_ID: u16 = 1;
pub const STREAM_SCHEMA_VERSION: u16 = 0;

pub mod template_id {
    pub const BEST_BID_ASK_STREAM_EVENT: u16 = 10001;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamDecodeError {
    BufferTooShort { expected: usize, actual: usize },
    GroupSizeTooLarge { count: usize, max: usize },
    InvalidUtf8,
    SchemaMismatch { expected: u16, actual: u16 },
    UnknownTemplateId(u16),
    InvalidBlockLength { expected: u16, actual: u16 },
}

impl Display for StreamDecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BufferTooShort { expected, actual } => {
                write!(f, "Buffer too short: expected {expected} bytes, was {actual}")
            }
            Self::GroupSizeTooLarge { count, max } => {
                write!(f, "Group size {count} exceeds maximum {max}")
            }
            Self::InvalidUtf8 => write!(f, "Invalid UTF-8 in symbol"),
            Self::SchemaMismatch { expected, actual } => {
                write!(f, "Schema mismatch: expected {expected}, was {actual}")
            }
            Self::UnknownTemplateId(id) => write!(f, "Unknown template ID: {id}"),
            Self::InvalidBlockLength { expected, actual } => {
                write!(f, "Invalid block length: expected {expected}, was {actual}")
            }
        }
    }
}

impl Error for StreamDecodeError {}

impl From<SbeDecodeError> for StreamDecodeError {
    fn from(err: SbeDecodeError) -> Self {
        match err {
            SbeDecodeError::BufferTooShort { expected, actual } => {
                Self::BufferTooShort { expected, actual }
            }
            SbeDecodeError::SchemaMismatch { expected, actual } => {
                Self::SchemaMismatch { expected, actual }
            }
            SbeDecodeError::VersionMismatch { .. } => Self::SchemaMismatch {
                expected: STREAM_SCHEMA_VERSION,
                actual: 0,
            },
            SbeDecodeError::UnknownTemplateId(id) => Self::UnknownTemplateId(id),
            SbeDecodeError::GroupSizeTooLarge { count, max } => Self::GroupSizeTooLarge {
                count: count as usize,
                max: max as usize,
            },
            SbeDecodeError::InvalidBlockLength { expected, actual } => {
                Self::InvalidBlockLength { expected, actual }
            }
            SbeDecodeError::InvalidUtf8 => Self::InvalidUtf8,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct MessageHeader {
    pub block_length: u16,
    pub template_id: u16,
    pub schema_id: u16,
    pub version: u16,
}

impl MessageHeader {
    pub const ENCODED_LENGTH: usize = 8;

    pub fn decode(buf: &[u8]) -> Result<Self, StreamDecodeError> {
        if buf.len() < Self::ENCODED_LENGTH {
            return Err(StreamDecodeError::BufferTooShort {
                expected: Self::ENCODED_LENGTH,
                actual: buf.len(),
            });
        }
        Ok(Self {
            block_length: u16::from_le_bytes([buf[0], buf[1]]),
            template_id: u16::from_le_bytes([buf[2], buf[3]]),
            schema_id: u16::from_le_bytes([buf[4], buf[5]]),
            version: u16::from_le_bytes([buf[6], buf[7]]),
        })
    }

    pub fn validate_schema(&self) -> Result<(), StreamDecodeError> {
        if self.schema_id != STREAM_SCHEMA_ID {
            return Err(StreamDecodeError::SchemaMismatch {
                expected: STREAM_SCHEMA_ID,
                actual: self.schema_id,
            });
        }
        Ok(())
    }
}

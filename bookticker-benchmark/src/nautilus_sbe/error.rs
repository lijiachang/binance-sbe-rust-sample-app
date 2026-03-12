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

//! Generic SBE decode error types.

use std::{error::Error, fmt::Display};

pub const MAX_GROUP_SIZE: u32 = 10_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SbeDecodeError {
    BufferTooShort { expected: usize, actual: usize },
    SchemaMismatch { expected: u16, actual: u16 },
    VersionMismatch { expected: u16, actual: u16 },
    UnknownTemplateId(u16),
    GroupSizeTooLarge { count: u32, max: u32 },
    InvalidBlockLength { expected: u16, actual: u16 },
    InvalidUtf8,
}

impl Display for SbeDecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BufferTooShort { expected, actual } => {
                write!(f, "Buffer too short: expected {expected} bytes, was {actual}")
            }
            Self::SchemaMismatch { expected, actual } => {
                write!(f, "Schema ID mismatch: expected {expected}, was {actual}")
            }
            Self::VersionMismatch { expected, actual } => {
                write!(f, "Schema version mismatch: expected {expected}, was {actual}")
            }
            Self::UnknownTemplateId(id) => write!(f, "Unknown template ID: {id}"),
            Self::GroupSizeTooLarge { count, max } => {
                write!(f, "Group size {count} exceeds maximum {max}")
            }
            Self::InvalidBlockLength { expected, actual } => {
                write!(f, "Invalid block length: expected {expected}, was {actual}")
            }
            Self::InvalidUtf8 => write!(f, "Invalid UTF-8 in string field"),
        }
    }
}

impl Error for SbeDecodeError {}

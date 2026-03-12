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

//! BestBidAsk stream event decoder.
//!
//! Message layout (after 8-byte header):
//! - eventTime: i64 (microseconds)
//! - bookUpdateId: i64
//! - priceExponent: i8
//! - qtyExponent: i8
//! - bidPrice: i64 (mantissa)
//! - bidQty: i64 (mantissa)
//! - askPrice: i64 (mantissa)
//! - askQty: i64 (mantissa)
//! - symbol: varString8

use super::{mantissa_to_f64, template_id, MessageHeader, StreamDecodeError};
use crate::nautilus_sbe::cursor::SbeCursor;

/// Best bid/ask stream event.
#[derive(Debug, Clone)]
pub struct BestBidAskStreamEvent {
    /// Event timestamp in microseconds.
    pub event_time_us: i64,
    /// Book update ID for sequencing.
    pub book_update_id: i64,
    /// Price exponent (prices = mantissa * 10^exponent).
    pub price_exponent: i8,
    /// Quantity exponent (quantities = mantissa * 10^exponent).
    pub qty_exponent: i8,
    /// Best bid price mantissa.
    pub bid_price_mantissa: i64,
    /// Best bid quantity mantissa.
    pub bid_qty_mantissa: i64,
    /// Best ask price mantissa.
    pub ask_price_mantissa: i64,
    /// Best ask quantity mantissa.
    pub ask_qty_mantissa: i64,
    /// Trading symbol.
    pub symbol: String,
}

impl BestBidAskStreamEvent {
    /// Fixed block length (excluding header and variable-length data).
    pub const BLOCK_LENGTH: usize = 50;

    /// Minimum buffer size needed (header + block + 1-byte string length).
    pub const MIN_BUFFER_SIZE: usize = MessageHeader::ENCODED_LENGTH + Self::BLOCK_LENGTH + 1;

    /// Decode from SBE buffer (including 8-byte header).
    ///
    /// # Errors
    ///
    /// Returns error if buffer is too short or contains invalid data.
    pub fn decode(buf: &[u8]) -> Result<Self, StreamDecodeError> {
        let header = MessageHeader::decode(buf)?;
        header.validate_schema()?;
        if header.template_id != template_id::BEST_BID_ASK_STREAM_EVENT {
            return Err(StreamDecodeError::UnknownTemplateId(header.template_id));
        }
        if header.block_length != Self::BLOCK_LENGTH as u16 {
            return Err(StreamDecodeError::InvalidBlockLength {
                expected: Self::BLOCK_LENGTH as u16,
                actual: header.block_length,
            });
        }
        Self::decode_validated(buf)
    }

    /// Decode from an SBE buffer whose header has already been validated.
    pub(crate) fn decode_validated(buf: &[u8]) -> Result<Self, StreamDecodeError> {
        let mut cursor = SbeCursor::new_at(buf, MessageHeader::ENCODED_LENGTH);
        Self::decode_body(&mut cursor)
    }

    #[inline]
    fn decode_body(cursor: &mut SbeCursor<'_>) -> Result<Self, StreamDecodeError> {
        let event_time_us = cursor.read_i64_le()?;
        let book_update_id = cursor.read_i64_le()?;
        let price_exponent = cursor.read_i8()?;
        let qty_exponent = cursor.read_i8()?;
        let bid_price_mantissa = cursor.read_i64_le()?;
        let bid_qty_mantissa = cursor.read_i64_le()?;
        let ask_price_mantissa = cursor.read_i64_le()?;
        let ask_qty_mantissa = cursor.read_i64_le()?;
        let symbol = cursor.read_var_string8()?;

        Ok(Self {
            event_time_us,
            book_update_id,
            price_exponent,
            qty_exponent,
            bid_price_mantissa,
            bid_qty_mantissa,
            ask_price_mantissa,
            ask_qty_mantissa,
            symbol,
        })
    }

    /// Get bid price as f64.
    #[inline]
    #[must_use]
    pub fn bid_price(&self) -> f64 {
        mantissa_to_f64(self.bid_price_mantissa, self.price_exponent)
    }

    /// Get bid quantity as f64.
    #[inline]
    #[must_use]
    pub fn bid_qty(&self) -> f64 {
        mantissa_to_f64(self.bid_qty_mantissa, self.qty_exponent)
    }

    /// Get ask price as f64.
    #[inline]
    #[must_use]
    pub fn ask_price(&self) -> f64 {
        mantissa_to_f64(self.ask_price_mantissa, self.price_exponent)
    }

    /// Get ask quantity as f64.
    #[inline]
    #[must_use]
    pub fn ask_qty(&self) -> f64 {
        mantissa_to_f64(self.ask_qty_mantissa, self.qty_exponent)
    }
}

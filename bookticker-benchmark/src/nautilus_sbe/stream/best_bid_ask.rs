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

use super::{template_id, MessageHeader, StreamDecodeError};
use crate::nautilus_sbe::cursor::SbeCursor;

#[derive(Debug, Clone)]
pub struct BestBidAskStreamEvent {
    pub event_time_us: i64,
    pub book_update_id: i64,
    pub price_exponent: i8,
    pub qty_exponent: i8,
    pub bid_price_mantissa: i64,
    pub bid_qty_mantissa: i64,
    pub ask_price_mantissa: i64,
    pub ask_qty_mantissa: i64,
    pub symbol: String,
}

impl BestBidAskStreamEvent {
    pub const BLOCK_LENGTH: usize = 50;
    pub const MIN_BUFFER_SIZE: usize = MessageHeader::ENCODED_LENGTH + Self::BLOCK_LENGTH + 1;

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
        let mut cursor = SbeCursor::new_at(buf, MessageHeader::ENCODED_LENGTH);
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
}

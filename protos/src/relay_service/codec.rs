use prost::bytes::{Buf, BufMut};
use tonic::Status;

use super::v1::RelayFrame;

#[derive(Default, Clone)]
pub(super) struct BytesCodec {}

impl tonic::codec::Codec for BytesCodec {
    type Encode = RelayFrame;
    type Decode = RelayFrame;

    type Encoder = RelayFrameEncoder;
    type Decoder = RelayFrameDecoder;

    fn encoder(&mut self) -> RelayFrameEncoder {
        RelayFrameEncoder
    }
    fn decoder(&mut self) -> RelayFrameDecoder {
        RelayFrameDecoder
    }
}

pub(super) struct RelayFrameEncoder;
pub(super) struct RelayFrameDecoder;

impl tonic::codec::Encoder for RelayFrameEncoder {
    type Item = RelayFrame;
    type Error = Status;

    fn encode(
        &mut self,
        item: Self::Item,
        dst: &mut tonic::codec::EncodeBuf<'_>,
    ) -> Result<(), Self::Error> {
        dst.put(item.payload);
        Ok(())
    }
}

impl tonic::codec::Decoder for RelayFrameDecoder {
    type Item = RelayFrame;
    type Error = Status;

    fn decode(
        &mut self,
        src: &mut tonic::codec::DecodeBuf<'_>,
    ) -> Result<Option<Self::Item>, Self::Error> {
        let payload = src.copy_to_bytes(src.remaining());
        Ok(if payload.is_empty() {
            None
        } else {
            Some(RelayFrame { payload })
        })
    }
}

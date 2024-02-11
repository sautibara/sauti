use crossbeam_channel::Receiver;

use crate::audio::prelude::*;
use crate::decoder::Decoder;
use crate::effect::prelude::*;

use super::Player;

#[derive(Clone)]
pub struct PacketPlayer<E: Effect> {
    receiver: Receiver<GenericPacket>,
    effects: E,
}

impl<E: Effect> PacketPlayer<E> {
    pub fn new<D: Decoder, A: Audio>(
        player: &Player<D, E, A>,
        receiver: Receiver<GenericPacket>,
    ) -> Self {
        Self {
            receiver,
            effects: player.effects.clone(),
        }
    }

    fn next_packet<S: ConvertibleSample>(&mut self, spec: &StreamSpec) -> SoundPacket<S> {
        let packet = self.receiver.recv().unwrap();
        let converted_packet = self.effects.apply_to_generic(packet, spec);
        converted_packet.convert::<S>()
    }
}

impl<E: Effect> SoundSource for PacketPlayer<E> {
    fn build<S: ConvertibleSample>(
        &self,
        info: DeviceInfo,
    ) -> impl FnMut(&mut [S]) + Send + 'static {
        let mut source = self.clone();
        let spec = info.into();

        let mut current_packet = None;
        let mut current_index = 0;

        move |channels| {
            let packet = current_packet.get_or_insert_with(|| source.next_packet(&spec));
            channels.copy_from_slice(
                &packet.interleaved_samples()[current_index..current_index + channels.len()],
            );
            current_index += channels.len();
            if current_index >= packet.frames() * packet.channels() {
                current_packet = None;
                current_index = 0;
            }
        }
    }
}

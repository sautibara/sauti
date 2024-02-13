use crossbeam_channel::Receiver;

use crate::audio::prelude::*;
use crate::decoder::Decoder;
use crate::effect::prelude::*;

use super::{AudioControl, Player};

#[derive(Clone)]
pub struct PacketPlayer<E: Effect> {
    packets: Receiver<GenericPacket>,
    audio_control: Receiver<AudioControl>,
    effects: E,
}

impl<E: Effect> PacketPlayer<E> {
    pub fn new<D: Decoder, A: Audio>(
        player: &Player<D, E, A>,
        packets: Receiver<GenericPacket>,
        audio_control: Receiver<AudioControl>,
    ) -> Self {
        Self {
            packets,
            audio_control,
            effects: player.effects.clone(),
        }
    }

    fn next_packet<S: ConvertibleSample>(&mut self, spec: &StreamSpec) -> SoundPacket<S> {
        let packet = self.packets.recv().unwrap();
        let converted_packet = self.effects.apply_to_generic(packet, spec);
        converted_packet.convert::<S>()
    }

    fn flush_recieved(&self) -> bool {
        matches!(self.audio_control.try_recv(), Ok(AudioControl::Flush))
    }
}

fn flush<T>(receiver: &Receiver<T>) {
    while receiver.try_recv().is_ok() {}
}

impl<E: Effect> SoundSource for PacketPlayer<E> {
    fn build<S: ConvertibleSample>(
        &self,
        info: DeviceInfo,
    ) -> impl FnMut(&mut [S]) + Send + 'static {
        let mut source = self.clone();
        let spec: StreamSpec = info.into();

        let mut current_packet = None;
        let mut current_index = 0;

        move |channels| {
            if source.flush_recieved() {
                flush(&source.packets);
                // current_packet = None;
                current_packet = Some(SoundPacket::from_interleaved(
                    vec![S::EQUILIBRIUM; spec.channels * 1000],
                    spec,
                ));
                source.effects.reset();
            }

            let packet = current_packet.get_or_insert_with(|| source.next_packet(&spec));

            if packet.frames() == 0 {
                current_packet = None;
                return;
            }
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

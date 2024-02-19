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

    fn flush_packets(&mut self) {
        while self.packets.try_recv().is_ok() {}
        self.effects.reset();
    }
}

impl<E: Effect> SoundSource for PacketPlayer<E> {
    fn build<S: ConvertibleSample>(&self, info: DeviceInfo) -> impl Sound<S> {
        PacketSound {
            receiver: self.clone(),
            current_packet: None,
            current_index: 0,
            spec: info.into(),
        }
    }
}

pub struct PacketSound<E: Effect, S: ConvertibleSample> {
    receiver: PacketPlayer<E>,
    current_packet: Option<SoundPacket<S>>,
    current_index: usize,
    spec: StreamSpec,
}

impl<E: Effect, S: ConvertibleSample> Sound<S> for PacketSound<E, S> {
    fn next_frame(&mut self, channels: &mut [S]) {
        if let Ok(message) = self.receiver.audio_control.try_recv() {
            self.handle(&message);
        }

        let packet =
            (self.current_packet).get_or_insert_with(|| self.receiver.next_packet(&self.spec));
        if packet.frames() == 0 {
            self.current_packet = None;
            return;
        }

        Self::copy_next_frame(packet, channels, self.current_index);
        let (channels, max_frames) = (packet.channels(), packet.frames());
        self.advance_index(channels, max_frames);
    }
}

impl<E: Effect, S: ConvertibleSample> PacketSound<E, S> {
    fn handle(&mut self, message: &AudioControl) {
        #[allow(clippy::single_match)] // more messages will be added
        match message {
            AudioControl::Flush => self.flush(),
            // _ => (),
        }
    }

    fn flush(&mut self) {
        self.receiver.flush_packets();
        // have some silence before it stops
        self.current_packet = Some(SoundPacket::from_interleaved(
            vec![S::EQUILIBRIUM; self.spec.channels * 1000],
            self.spec,
        ));
    }

    fn copy_next_frame(packet: &SoundPacket<S>, channels: &mut [S], index: usize) {
        channels.copy_from_slice(&packet.interleaved_samples()[index..index + channels.len()]);
    }

    fn advance_index(&mut self, channels: usize, max_frames: usize) {
        self.current_index += channels;
        if self.current_index >= channels * max_frames {
            self.current_packet = None;
            self.current_index = 0;
        }
    }
}

use crossbeam_channel::{select, Receiver};

use crate::decoder::Decoder;
use crate::effect::prelude::*;
use crate::{effect::List, output::prelude::*};

use super::on_file_end::OnFileEnd;
use super::{OutputControl, Player};

#[derive(Clone)]
pub struct PacketPlayer<E: Effect> {
    packets: Receiver<GenericPacket>,
    output_control: Receiver<OutputControl>,
    effects: List<E, effect::ConstantVolume>,
}

impl<E: Effect> PacketPlayer<E> {
    pub fn new<D: Decoder, O: Output, C: OnFileEnd>(
        player: &Player<O, D, E, C>,
        packets: Receiver<GenericPacket>,
        output_control: Receiver<OutputControl>,
        volume: f64,
    ) -> Self {
        Self {
            packets,
            output_control,
            effects: (player.effects.clone()).then(effect::ConstantVolume(volume)),
        }
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
            // the player starts out paused until it recieves a song to play
            playing: false,
        }
    }
}

pub struct PacketSound<E: Effect, S: ConvertibleSample> {
    receiver: PacketPlayer<E>,
    current_packet: Option<SoundPacket<S>>,
    current_index: usize,
    spec: StreamSpec,
    playing: bool,
}

impl<E: Effect, S: ConvertibleSample> Sound<S> for PacketSound<E, S> {
    fn next_frame(&mut self, channels: &mut [S]) {
        if let Ok(message) = self.receiver.output_control.try_recv() {
            self.handle(&message);
        }

        let index = self.current_index;
        loop {
            // nothing's playing, the sound could just return
            // this has to get checked each loop for if the player pauses
            if !self.playing {
                channels.fill(S::EQUILIBRIUM);
                return;
            }

            // get the next packet or control signal
            match self.packet() {
                // there was a packet! play it
                Ok(packet) => {
                    Self::copy_next_frame(packet, channels, index);
                    let (channels, max_frames) = (packet.channels(), packet.frames());
                    self.advance_index(channels, max_frames);
                    return;
                }
                // there wasn't a packet. Handle the signal, and keep trying to find one.
                Err(control) => {
                    self.handle(&control);
                }
            }
        }
    }
}

impl<E: Effect, S: ConvertibleSample> PacketSound<E, S> {
    fn next_packet(&mut self) -> Result<SoundPacket<S>, OutputControl> {
        select! {
            recv(self.receiver.output_control) -> control => {
                let control = control.expect("the output control sender should never hang up before exiting");
                Err(control)
            },
            recv(self.receiver.packets) -> packet => {
                let packet = packet.expect("the packet sender should never hang up before exiting");
                let effected = self.receiver.effects
                    .apply_to_generic(packet, &self.spec);
                Ok(effected.convert())
            },
        }
    }

    // This is essentially a call of get_or_insert, but it couldn't be used. The lifetimes of
    // &self.current_packet and &mut self would interfere, and there's a chance of of an
    // AudioControl coming up.
    fn packet(&mut self) -> Result<&SoundPacket<S>, OutputControl> {
        if self.current_packet.is_none() {
            let packet = self.next_packet()?;
            self.current_packet = Some(packet);
        }

        // SAFETY: checked if the packet was None above
        Ok(unsafe { self.current_packet.as_ref().unwrap_unchecked() })
    }

    fn handle(&mut self, message: &OutputControl) {
        #[allow(clippy::single_match)] // more messages will be added
        match message {
            OutputControl::Flush => self.flush(),
            OutputControl::SetState(val) => self.playing = val.is_playing(),
            // the volume setter is at the end of the list
            OutputControl::SetVolume(val) => self.receiver.effects.after().0 = *val,
        }
    }

    fn flush(&mut self) {
        self.receiver.flush_packets();
        self.current_packet = None;
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

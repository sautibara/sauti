use std::sync::Arc;
use std::thread;
use std::time::Duration;

#[cfg(feature = "decoder")]
use crate::decoder::prelude::*;
#[cfg(feature = "effect")]
use crate::effect::prelude::*;
#[cfg(feature = "output")]
use crate::output::prelude::*;
#[cfg(feature = "player")]
use crate::player::callback::prelude::*;
#[cfg(feature = "player")]
use crate::player::prelude::*;

const NANOS_PER_SEC: u64 = 1_000_000_000;

/// Provides implementations for [`Output`], [`Decoder`], [`Effect`], [`OnStreamEnd`], and [`OnError`] that all do nothing
#[derive(Clone, Copy)]
pub struct Empty;

impl Empty {
    /// Create a [`PlayerBuilder`](crate::player::builder::Builder) that uses all [`Empty`]
    /// implementations
    ///
    /// # Examples
    ///
    /// ```
    /// # fn main() -> Result<(), sauti::player::Disconnected> {
    /// use sauti::test::prelude::*;
    ///
    /// let handle = Empty::player().run();
    /// // none of these actually result in audio being played
    /// handle.play("")?;
    /// handle.pause()?;
    /// handle.resume()?;
    /// # Ok(()) }
    /// ```
    #[must_use]
    #[cfg(feature = "player")]
    pub fn player() -> crate::player::builder::Builder<Self, Self, Self, Self, Self> {
        Player::builder()
            .decoder(Self)
            .output(Self)
            .effects(Self)
            .on_error(Self)
            .on_stream_end(Self)
    }

    #[cfg(feature = "output")]
    fn drain_source<S: SoundSource>(source: &S, info: DeviceInfo) {
        let mut sound = source.build(info);
        thread::spawn(move || {
            let mut vec = vec![f32::EQUILIBRIUM; info.channels];
            loop {
                sound.next_frame(&mut vec[..]);
                // sleep a little so the computer doesn't blow up
                std::thread::sleep(Duration::from_nanos(
                    NANOS_PER_SEC / info.sample_rate as u64,
                ));
            }
        });
    }
}

#[cfg(feature = "output")]
impl Output for Empty {
    fn start<S: SoundSource>(
        &self,
        options: impl Into<DeviceOptions>,
        source: S,
    ) -> OutputResult<Box<dyn Device>> {
        let info = DeviceInfo::default().apply(&options.into());
        // the source must be run in some way,
        // so just ignore everything it says
        Self::drain_source(&source, info);
        Ok(Box::new(EmptyDevice { info }))
    }
}

#[cfg(feature = "output")]
struct EmptyDevice {
    info: DeviceInfo,
}

#[cfg(feature = "output")]
impl Device for EmptyDevice {
    fn info(&self) -> &DeviceInfo {
        &self.info
    }

    fn pause(&mut self) -> OutputResult<()> {
        Ok(())
    }

    fn resume(&mut self) -> OutputResult<()> {
        Ok(())
    }

    fn restart(&mut self) -> OutputResult<()> {
        Ok(())
    }

    fn inner_modify_options(
        &mut self,
        options: DeviceOptions,
    ) -> OutputResult<Option<Box<dyn Device>>> {
        self.info = self.info.apply(&options);
        Ok(None)
    }
}

#[cfg(feature = "decoder")]
impl Decoder for Empty {
    fn read(&self, _source: &MediaSource) -> DecoderResult<Box<dyn AudioStream>> {
        Ok(Box::new(Self))
    }
}

#[cfg(feature = "decoder")]
impl AudioStream for Empty {
    fn next_packet(&mut self) -> DecoderResult<Option<GenericPacket>> {
        Ok(None)
    }

    fn seek_to(&mut self, _duration: std::time::Duration) -> DecoderResult<()> {
        Ok(())
    }

    fn seek_by(
        &mut self,
        _duration: std::time::Duration,
        _direction: crate::decoder::Direction,
    ) -> DecoderResult<()> {
        Ok(())
    }

    fn source(&self) -> &SourceName {
        &SourceName::Unknown
    }

    fn position(&self) -> Duration {
        Duration::ZERO
    }

    fn duration(&self) -> Duration {
        Duration::ZERO
    }

    fn times(&self) -> Arc<dyn StreamTimes> {
        Arc::new(Self)
    }
}

#[cfg(feature = "decoder")]
impl StreamTimes for Empty {
    fn duration(&self) -> Duration {
        Duration::ZERO
    }

    fn position(&self) -> Duration {
        Duration::ZERO
    }
}

#[cfg(feature = "effect")]
impl Effect for Empty {
    fn apply_to<S: ConvertibleSample>(
        &mut self,
        input: SoundPacket<S>,
        _output_spec: &StreamSpec,
    ) -> SoundPacket<S> {
        input
    }
}

#[cfg(feature = "player")]
impl OnError for Empty {
    fn handle(&self, _: PlayerError, _: PlayerRef) -> impl Into<callback::ActionSet> {
        None
    }
}

#[cfg(feature = "player")]
impl OnStreamEnd for Empty {
    fn stream_ended(&self, _: callback::stream_end::Info<'_>) -> PlayerResult<()> {
        Ok(())
    }
}

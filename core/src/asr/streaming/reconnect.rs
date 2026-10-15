use std::collections::VecDeque;

use tokio::sync::{mpsc, oneshot};

use super::StreamingInput;

const MAX_BUFFERED_SAMPLES: usize = 16_000 * 30;

/// Pipeline input held while reconnecting, so capture never blocks on a dead socket.
#[derive(Default)]
pub(super) struct ReconnectBuffer {
    pub inputs: VecDeque<StreamingInput>,
    samples: usize,
    pub closed: bool,
}

impl ReconnectBuffer {
    fn push(&mut self, input: StreamingInput) {
        match input {
            StreamingInput::Commit(result) => {
                // Acknowledge now and keep the utterance boundary for replay.
                let _ = result.send(Ok(()));
                self.inputs
                    .push_back(StreamingInput::Commit(oneshot::channel().0));
            }
            StreamingInput::FinishQuestion(result) => {
                let _ = result.send(Err("Question recognition session reconnected".into()));
            }
            input @ (StreamingInput::Audio(_) | StreamingInput::AudioActivity(_, _)) => {
                self.samples += audio_len(&input);
                self.inputs.push_back(input);
                if self.samples > MAX_BUFFERED_SAMPLES {
                    tracing::warn!(
                        "Cloud recognition reconnect buffer is full; dropping oldest audio"
                    );
                }
                while self.samples > MAX_BUFFERED_SAMPLES {
                    let Some(dropped) = self.inputs.pop_front() else {
                        break;
                    };
                    self.samples -= audio_len(&dropped);
                }
            }
        }
    }
}

fn audio_len(input: &StreamingInput) -> usize {
    match input {
        StreamingInput::Audio(samples) | StreamingInput::AudioActivity(samples, _) => samples.len(),
        _ => 0,
    }
}

/// Runs `future` while draining pipeline input into `buffer`.
pub(super) async fn buffer_until<F: std::future::Future>(
    future: F,
    audio: &mut mpsc::Receiver<StreamingInput>,
    buffer: &mut ReconnectBuffer,
) -> F::Output {
    tokio::pin!(future);
    loop {
        tokio::select! {
            result = &mut future => return result,
            input = audio.recv(), if !buffer.closed => match input {
                Some(input) => buffer.push(input),
                None => buffer.closed = true,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[tokio::test]
    async fn commits_are_acknowledged_and_oldest_audio_is_dropped_when_full() {
        let mut buffer = ReconnectBuffer::default();
        let (commit_tx, commit_rx) = oneshot::channel();
        buffer.push(StreamingInput::Audio(Arc::new(vec![
            0.0;
            MAX_BUFFERED_SAMPLES
        ])));
        buffer.push(StreamingInput::Commit(commit_tx));
        buffer.push(StreamingInput::Audio(Arc::new(vec![0.0; 512])));

        assert_eq!(commit_rx.await.unwrap(), Ok(()));
        assert_eq!(buffer.samples, 512);
        assert!(matches!(
            buffer.inputs.iter().collect::<Vec<_>>().as_slice(),
            [StreamingInput::Commit(_), StreamingInput::Audio(_)]
        ));
    }
}

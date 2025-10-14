// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{
    fmt,
    marker::PhantomData,
    pin::Pin,
    sync::Arc,
    time::{Duration, Instant},
};

use tokio::time;
use tokio_stream::{Stream, StreamExt};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info};
use uuid::Uuid;

use super::{FibonacciBackoff, spawn_from_sync};

/// Timeout after a stream stop is not considered as error
const DEFAULT_REGULAR_STOP_TIMEOUT: Duration = Duration::from_secs(30 * 60 * 60); // 30 minutes

/// A task that runs in the background and handles events from a stream.
///
/// The task is created from a [`BackgroundStreamContext`], which is responsible for creating the
/// stream and handling events. The task is responsible for managing the lifetime of the stream:
///
/// - stream is created by the [`BackgroundStreamContext`]
/// - stream is created when the app is in the foreground
/// - stream is stopped when the app goes in the background
/// - stream is restarted on errors with a fibonacci backoff
/// - stream is restarted when it gracefully stops
pub(crate) struct BackgroundStreamTask<C, Event> {
    id: Uuid,
    name: Arc<str>,
    context: C,
    cancel: CancellationToken,
    /// Timeout after a stream stop is not considered as an error
    regular_stop_timeout: Duration,
    backoff: FibonacciBackoff,
    state: State<Event>,
    _marker: PhantomData<Event>,
}

impl<C, Event> BackgroundStreamTask<C, Event>
where
    C: BackgroundStreamContext<Event> + Send + 'static,
    Event: fmt::Debug + Send + 'static,
{
    pub(crate) fn new(name: impl Into<String>, context: C, cancel: CancellationToken) -> Self {
        let id = Uuid::new_v4();
        let name = name.into().into();
        debug!(%name, %id, "new background stream task");
        Self {
            id,
            name,
            context,
            cancel,
            state: State::Initial,
            regular_stop_timeout: DEFAULT_REGULAR_STOP_TIMEOUT,
            backoff: FibonacciBackoff::new(),
            _marker: PhantomData,
        }
    }

    #[cfg(test)]
    fn with_regular_stop_timeout(mut self, value: Duration) -> Self {
        self.regular_stop_timeout = value;
        self
    }

    /// Does one state transition.
    ///
    /// In case the future is cancelled, the state is set to `Finished`.
    ///
    /// # State transitions
    ///
    /// ```text
    /// Initial --> Running
    ///   ^       |
    ///   |       v
    ///   +-------Stopped --(after timeout)--> Initial
    ///   |       |
    ///   |       |--(error/before timeout)--> Backoff --(timeout)--> Initial
    ///   |       |
    ///   |    (cancelled)
    ///   |       |
    ///   +-------Finished
    /// ```
    async fn step(&mut self) {
        self.state = match std::mem::take(&mut self.state) {
            State::Finished => State::Finished,

            _ if self.cancel.is_cancelled() => {
                info!(name = %self.name, id = %self.id, "background stream stopped");
                State::Finished
            }

            // Transition from initial to running state after the app is in the foreground.
            State::Initial => {
                self.context.in_foreground().await;

                let started_at = Instant::now();
                info!(name = %self.name, id = %self.id, "background stream starting");
                match self.context.create_stream().await {
                    Ok(stream) => {
                        info!(name = %self.name, id = %self.id, "background stream started");
                        self.backoff.reset();
                        State::Running {
                            stream: Box::pin(stream),
                            started_at,
                        }
                    }
                    Err(e) => State::Backoff {
                        error: Some(e),
                        timeout: self.backoff.next_backoff(),
                    },
                }
            }

            // Get next event from the stream and handle it.
            State::Running {
                mut stream,
                started_at,
            } => {
                enum NextEvent<Event> {
                    Event(Option<Event>),
                    Cancelled,
                    InBackground,
                }

                let event = tokio::select! {
                    event = stream.next() => NextEvent::Event(event),
                    _ = self.cancel.cancelled() => NextEvent::Cancelled,
                    _ = self.context.in_background() => NextEvent::InBackground,
                };

                match event {
                    NextEvent::Event(Some(event)) => {
                        debug!(name = %self.name, id = %self.id, ?event, "received event");
                        let should_continue = self.context.handle_event(event).await;
                        if !should_continue {
                            State::Stopped { started_at }
                        } else {
                            self.backoff.reset();
                            State::Running { stream, started_at }
                        }
                    }
                    // stream exhausted
                    NextEvent::Event(None) => State::Stopped { started_at },
                    NextEvent::InBackground => State::Initial,
                    NextEvent::Cancelled => State::Finished,
                }
            }

            State::Stopped { started_at } => {
                if started_at.elapsed() >= self.regular_stop_timeout {
                    // reset backoff after a regular stop timeout
                    self.backoff.reset();
                    State::Initial
                } else {
                    // if failed faster than regular stop, retry with backoff
                    State::Backoff {
                        error: None,
                        timeout: self.backoff.next_backoff(),
                    }
                }
            }

            // Waits for the backoff to expire before starting again.
            State::Backoff { error, timeout } => {
                if let Some(error) = error {
                    error!(
                        name = %self.name,
                        id = %self.id,
                        retry_in =? timeout,
                        %error,
                        "background stream backoff"
                    );
                } else {
                    info!(
                        name = %self.name,
                        id = %self.id,
                        retry_in =? timeout,
                        "background stream backoff"
                    );
                }
                time::sleep(timeout).await;
                State::Initial
            }
        };
    }

    pub(crate) fn spawn(mut self) {
        spawn_from_sync(async move {
            while !matches!(self.state, State::Finished) {
                self.step().await;
            }
        });
    }
}

type EventStream<Event> = Pin<Box<dyn Stream<Item = Event> + Send + 'static>>;

enum State<Event> {
    /// Initial state
    ///
    /// Creates an event stream and transitions to `Running` when the app is in the foreground.
    Initial,
    /// The event stream is created and events are being fetched and handled in each step.
    Running {
        stream: EventStream<Event>,
        started_at: Instant,
    },
    /// The event stream has been stopped with an error or gracefully.
    Stopped { started_at: Instant },
    /// The event stream has failed or stopped too fast, and is waiting for a backoff period before
    /// retrying.
    Backoff {
        error: Option<anyhow::Error>,
        timeout: Duration,
    },
    /// The task has been cancelled and is finished.
    Finished,
}

impl<Event: fmt::Debug> fmt::Debug for State<Event> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Initial => write!(f, "Initial"),
            Self::Running {
                stream: _,
                started_at,
            } => f
                .debug_struct("Running")
                .field("started_at", started_at)
                .finish_non_exhaustive(),
            Self::Stopped { started_at } => f
                .debug_struct("Stopped")
                .field("started_at", started_at)
                .finish(),
            Self::Backoff { error, timeout } => f
                .debug_struct("Backoff")
                .field("error", error)
                .field("timeout", timeout)
                .finish(),
            Self::Finished => write!(f, "Finished"),
        }
    }
}

impl<Event> Default for State<Event> {
    fn default() -> Self {
        Self::Finished
    }
}

/// Context for a background stream task.
///
/// Responsible for creating the stream and handling events.
pub(crate) trait BackgroundStreamContext<Event>: Send {
    /// Create the backtrack stream
    fn create_stream(
        &mut self,
    ) -> impl Future<Output = anyhow::Result<impl Stream<Item = Event> + Send + 'static>> + Send;

    /// Handle a stream event
    ///
    /// Returns `true` if the stream should continue to be processed, otherwise
    /// the stream should be stopped.
    fn handle_event(&mut self, event: Event) -> impl Future<Output = bool> + Send;

    /// Resolves when the app is in the foreground
    fn in_foreground(&self) -> impl Future<Output = ()> + Send;

    /// Resolves when the app is in the background
    fn in_background(&self) -> impl Future<Output = ()> + Send;
}

#[cfg(test)]
mod test {
    use anyhow::anyhow;
    use tokio::{
        sync::{Mutex, mpsc, oneshot, watch},
        time::timeout,
    };
    use tokio_stream::wrappers::ReceiverStream;

    use crate::init_test_tracing;

    use super::*;

    #[derive(Debug)]
    struct TestEvent {
        value: u64,
        ack_tx: oneshot::Sender<u64>,
    }

    impl TestEvent {
        fn new(value: u64) -> (Self, impl Future<Output = anyhow::Result<u64>>) {
            let (ack_tx, ack_rx) = oneshot::channel();
            let ack = async move { Ok(timeout(Duration::from_millis(100), ack_rx).await??) };
            (Self { value, ack_tx }, ack)
        }
    }

    enum AppState {
        Background,
        Foreground,
    }

    type TestStream = ReceiverStream<TestEvent>;

    struct TestContext {
        app_state_rx: watch::Receiver<AppState>,
        create_stream_rx: Arc<Mutex<mpsc::Receiver<anyhow::Result<TestStream>>>>,
    }

    impl TestContext {
        fn new() -> (
            Self,
            watch::Sender<AppState>,
            mpsc::Sender<anyhow::Result<TestStream>>,
        ) {
            let (app_state_tx, app_state_rx) = watch::channel(AppState::Foreground);
            let (create_stream_tx, create_stream_rx) = mpsc::channel(1);
            (
                Self {
                    app_state_rx,
                    create_stream_rx: Arc::new(Mutex::new(create_stream_rx)),
                },
                app_state_tx,
                create_stream_tx,
            )
        }
    }

    impl BackgroundStreamContext<TestEvent> for TestContext {
        fn create_stream(
            &mut self,
        ) -> impl Future<Output = anyhow::Result<impl Stream<Item = TestEvent> + 'static>> {
            let rx = self.create_stream_rx.clone();
            async move {
                let stream = rx.lock().await.recv().await.unwrap()?;
                Ok(stream)
            }
        }

        async fn handle_event(&mut self, event: TestEvent) -> bool {
            let _ = event.ack_tx.send(event.value);
            true
        }

        async fn in_foreground(&self) {
            let _ = self
                .app_state_rx
                .clone()
                .wait_for(|app_state| matches!(app_state, AppState::Foreground))
                .await;
        }

        async fn in_background(&self) {
            let _ = self
                .app_state_rx
                .clone()
                .wait_for(|app_state| matches!(app_state, AppState::Background))
                .await;
        }
    }

    macro_rules! assert_state {
        ($state:expr, $expected:pat $(if $guard:expr)?) => {
            assert!(matches!($state, $expected $(if $guard)?), "current state = {:?}", $state);
        };
    }

    async fn step_with_timeout(task: &mut BackgroundStreamTask<TestContext, TestEvent>) {
        timeout(Duration::from_secs(2), task.step()).await.unwrap()
    }

    #[tokio::test]
    async fn background_stream_task_handler_works() {
        init_test_tracing();

        let (context, _app_state_tx, create_stream_tx) = TestContext::new();

        let cancel = CancellationToken::new();
        let mut task = BackgroundStreamTask::new("test", context, cancel);
        assert_state!(task.state, State::Initial);

        let (event_tx, event_rx) = mpsc::channel(1);
        create_stream_tx
            .send(Ok(ReceiverStream::new(event_rx)))
            .await
            .unwrap();

        step_with_timeout(&mut task).await;
        assert_state!(task.state, State::Running { .. });

        let (event, ack) = TestEvent::new(1);
        event_tx.send(event).await.unwrap();
        step_with_timeout(&mut task).await;
        assert_state!(task.state, State::Running { .. });
        assert_eq!(ack.await.unwrap(), 1);

        let (event, ack) = TestEvent::new(2);
        event_tx.send(event).await.unwrap();
        step_with_timeout(&mut task).await;
        assert_state!(task.state, State::Running { .. });
        assert_eq!(ack.await.unwrap(), 2);
    }

    #[tokio::test]
    async fn background_stream_task_regular_stop() {
        init_test_tracing();

        let (context, _app_state_tx, create_stream_tx) = TestContext::new();

        let cancel = CancellationToken::new();
        let mut task = BackgroundStreamTask::new("test", context, cancel);
        assert_state!(task.state, State::Initial);

        let (event_tx, event_rx) = mpsc::channel(1);
        create_stream_tx
            .send(Ok(ReceiverStream::new(event_rx)))
            .await
            .unwrap();

        step_with_timeout(&mut task).await;
        assert_state!(task.state, State::Running { .. });

        let (event, ack) = TestEvent::new(1);
        event_tx.send(event).await.unwrap();
        step_with_timeout(&mut task).await;
        assert_state!(task.state, State::Running { .. });
        assert_eq!(ack.await.unwrap(), 1);

        drop(event_tx); // close stream

        step_with_timeout(&mut task).await;
        assert_state!(task.state, State::Stopped { .. });

        step_with_timeout(&mut task).await;
        assert_state!(
            task.state,
            State::Backoff {
                error: None,
                timeout
            } if timeout == Duration::from_secs(1)
        );

        step_with_timeout(&mut task).await;
        assert_state!(task.state, State::Initial);

        let (event_tx, event_rx) = mpsc::channel(1);
        create_stream_tx
            .send(Ok(ReceiverStream::new(event_rx)))
            .await
            .unwrap();

        step_with_timeout(&mut task).await;
        assert_state!(task.state, State::Running { .. });

        let (event, ack) = TestEvent::new(2);
        event_tx.send(event).await.unwrap();
        step_with_timeout(&mut task).await;
        assert_state!(task.state, State::Running { .. });
        assert_eq!(ack.await.unwrap(), 2);
    }

    #[tokio::test]
    async fn background_stream_task_regular_stop_after_timeout() {
        init_test_tracing();

        let (context, _app_state_tx, create_stream_tx) = TestContext::new();

        let cancel = CancellationToken::new();
        let mut task = BackgroundStreamTask::new("test", context, cancel)
            .with_regular_stop_timeout(Duration::from_millis(100));

        assert_state!(task.state, State::Initial);

        let (event_tx, event_rx) = mpsc::channel(1);
        create_stream_tx
            .send(Ok(ReceiverStream::new(event_rx)))
            .await
            .unwrap();

        step_with_timeout(&mut task).await;
        assert_state!(task.state, State::Running { .. });

        // sleep to be after the timeout
        time::sleep(Duration::from_millis(100)).await;
        // increase backoff to test that it will be reset
        let _ = task.backoff.next_backoff();
        drop(event_tx); // close stream

        step_with_timeout(&mut task).await;
        assert_state!(task.state, State::Stopped { .. });

        step_with_timeout(&mut task).await;
        assert_state!(task.state, State::Initial); // No backoff
        assert_eq!(task.backoff.next_backoff(), Duration::from_secs(1));
    }

    #[tokio::test]
    async fn background_stream_task_create_stream_error() {
        init_test_tracing();

        let (context, _app_state_tx, create_stream_tx) = TestContext::new();
        let cancel = CancellationToken::new();
        let mut task = BackgroundStreamTask::new("test", context, cancel);
        assert_state!(task.state, State::Initial);

        let err = anyhow!("Stream creation failed");
        create_stream_tx.send(Err(err)).await.unwrap();

        step_with_timeout(&mut task).await;
        assert_state!(
            task.state,
            State::Backoff {
                error: Some(_),
                timeout
            } if timeout == Duration::from_secs(1)
        );

        step_with_timeout(&mut task).await;
        assert_state!(task.state, State::Initial);

        let (event_tx, event_rx) = mpsc::channel(1);
        create_stream_tx
            .send(Ok(ReceiverStream::new(event_rx)))
            .await
            .unwrap();
        step_with_timeout(&mut task).await;
        assert_state!(task.state, State::Running { .. });

        let (event, ack) = TestEvent::new(1);
        event_tx.send(event).await.unwrap();
        step_with_timeout(&mut task).await;
        assert_state!(task.state, State::Running { .. });
        assert_eq!(ack.await.unwrap(), 1);
    }

    #[tokio::test]
    async fn background_stream_task_initial_cancel() {
        init_test_tracing();

        let (context, _app_state_tx, _create_stream_tx) = TestContext::new();
        let cancel = CancellationToken::new();
        let mut task = BackgroundStreamTask::new("test", context, cancel.clone());
        assert_state!(task.state, State::Initial);

        cancel.cancel();

        step_with_timeout(&mut task).await;
        assert_state!(task.state, State::Finished);
    }

    #[tokio::test]
    async fn background_stream_task_cancel_after_stream_creation() {
        init_test_tracing();

        let (context, _app_state_tx, create_stream_tx) = TestContext::new();
        let cancel = CancellationToken::new();
        let mut task = BackgroundStreamTask::new("test", context, cancel.clone());
        assert_state!(task.state, State::Initial);

        let (_event_tx, event_rx) = mpsc::channel(1);
        create_stream_tx
            .send(Ok(ReceiverStream::new(event_rx)))
            .await
            .unwrap();

        step_with_timeout(&mut task).await;
        assert_state!(task.state, State::Running { .. });

        cancel.cancel();

        step_with_timeout(&mut task).await;
        assert_state!(task.state, State::Finished);
    }

    #[tokio::test]
    async fn background_stream_task_running_to_background() {
        init_test_tracing();

        let (context, app_state_tx, create_stream_tx) = TestContext::new();
        let cancel = CancellationToken::new();
        let mut task = BackgroundStreamTask::new("test", context, cancel.clone());
        assert_state!(task.state, State::Initial);

        let (_event_tx, event_rx) = mpsc::channel(1);
        create_stream_tx
            .send(Ok(ReceiverStream::new(event_rx)))
            .await
            .unwrap();

        step_with_timeout(&mut task).await;
        assert_state!(task.state, State::Running { .. });

        app_state_tx.send(AppState::Background).unwrap();

        step_with_timeout(&mut task).await;
        assert_state!(task.state, State::Initial);
    }

    #[tokio::test]
    async fn background_stream_task_backoff_increases() {
        init_test_tracing();

        let (context, _app_state_tx, create_stream_tx) = TestContext::new();
        let cancel = CancellationToken::new();
        let mut task = BackgroundStreamTask::new("test", context, cancel.clone());
        assert_state!(task.state, State::Initial);

        create_stream_tx
            .send(Err(anyhow!("Stream creation failed")))
            .await
            .unwrap();

        step_with_timeout(&mut task).await;
        assert_state!(
            task.state,
            State::Backoff {
                error: Some(_),
                timeout
            }
            if timeout == Duration::from_secs(1)
        );

        create_stream_tx
            .send(Err(anyhow!("Stream creation failed")))
            .await
            .unwrap();

        step_with_timeout(&mut task).await;
        assert_state!(task.state, State::Initial);

        step_with_timeout(&mut task).await;
        assert_state!(
            task.state,
            State::Backoff {
                error: Some(_),
                timeout
            }
            if timeout == Duration::from_secs(2)
        );
    }

    #[tokio::test]
    async fn background_stream_task_backoff_resets() {
        init_test_tracing();

        let (context, _app_state_tx, create_stream_tx) = TestContext::new();
        let cancel = CancellationToken::new();
        let mut task = BackgroundStreamTask::new("test", context, cancel.clone());
        assert_state!(task.state, State::Initial);

        create_stream_tx
            .send(Err(anyhow!("Stream creation failed")))
            .await
            .unwrap();

        step_with_timeout(&mut task).await;
        assert_state!(
            task.state,
            State::Backoff {
                error: Some(_),
                timeout
            }
            if timeout == Duration::from_secs(1)
        );

        step_with_timeout(&mut task).await;
        assert_state!(task.state, State::Initial);

        let (event_tx, event_rx) = mpsc::channel(1);
        create_stream_tx
            .send(Ok(ReceiverStream::new(event_rx)))
            .await
            .unwrap();

        step_with_timeout(&mut task).await;
        assert_state!(task.state, State::Running { .. });

        let (event, ack) = TestEvent::new(1);
        event_tx.send(event).await.unwrap();
        step_with_timeout(&mut task).await;
        assert_state!(task.state, State::Running { .. });
        assert_eq!(ack.await.unwrap(), 1);
        assert_eq!(task.backoff.next_backoff(), Duration::from_secs(1));
    }

    #[tokio::test]
    async fn background_stream_task_waits_initially_for_foreground() {
        init_test_tracing();

        let (context, app_state_tx, _create_stream_tx) = TestContext::new();
        let cancel = CancellationToken::new();
        let mut task = BackgroundStreamTask::new("test", context, cancel.clone());
        assert_state!(task.state, State::Initial);

        app_state_tx.send(AppState::Background).unwrap();

        timeout(Duration::from_millis(1100), task.step())
            .await
            .expect_err("should timeout because in background");
        assert_state!(task.state, State::Finished); // the future was cancelled
    }
}

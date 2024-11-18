use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use futures::future::select_all;
use tokio::process::Command as TokioCommand;
use tokio::sync::Mutex;

pub type EventHandlerFn =
    Box<dyn FnOnce() -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>> + Send>;

pub trait Listener: Send + Sync {
    fn set_process_env(&self, process: &mut TokioCommand) -> Result<(), String>;
    fn next(&mut self) -> Pin<Box<dyn Future<Output = (EventHandlerFn, bool)> + Send + '_>>;
    fn stop(&mut self) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send + '_>>;
}

type ListenerActiveFuture = Pin<Box<dyn Future<Output = (EventHandlerFn, bool)> + Send>>;

#[derive(Default)]
pub struct ListenerManager {
    listeners: Vec<Arc<Mutex<Box<dyn Listener>>>>,
    active_futures: HashMap<usize, ListenerActiveFuture>,
    started: bool,
}

impl Clone for ListenerManager {
    fn clone(&self) -> Self {
        Self {
            listeners: self.listeners.clone(),
            active_futures: HashMap::new(),
            started: false,
        }
    }
}

impl std::fmt::Debug for ListenerManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(
            f,
            "ListenerManager {{ started: {}, listeners: {:?} }}",
            self.started,
            self.listeners.len()
        )
    }
}

impl ListenerManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_listener(&mut self, listener: Box<dyn Listener>) {
        let index = self.listeners.len();
        let listener = Arc::new(Mutex::new(listener));

        // Start the future immediately if we add a listener after start()
        if self.started {
            let listener_clone = listener.clone();
            let future = Box::pin(async move {
                let mut lock = listener_clone.lock().await;
                lock.next().await
            });
            self.active_futures.insert(index, future);
        }

        self.listeners.push(listener);
    }

    pub async fn set_process_env(&self, process: &mut TokioCommand) -> Result<(), String> {
        for listener in &self.listeners {
            let lock = listener.lock().await;
            lock.set_process_env(process)?;
        }
        Ok(())
    }

    pub fn start(&mut self) {
        for (index, listener) in self.listeners.iter().enumerate() {
            let listener_clone = listener.clone();
            let future = Box::pin(async move {
                let mut lock = listener_clone.lock().await;
                lock.next().await
            });
            self.active_futures.insert(index, future);
        }
        self.started = true;
    }

    pub async fn next(&mut self) -> Option<(EventHandlerFn, bool)> {
        // If no futures are active, return None
        if !self.started || self.active_futures.is_empty() {
            return None;
        }

        let futures: Vec<_> = self
            .active_futures
            .iter_mut()
            .map(|(&idx, fut)| (idx, fut.as_mut()))
            .collect();

        let indices: Vec<_> = futures.iter().map(|(idx, _)| *idx).collect();
        let futures: Vec<_> = futures.into_iter().map(|(_, fut)| fut).collect();

        let ((handler, interactive), index, _remaining) = select_all(futures).await;
        let listener_index = indices[index];

        // Remove the completed future
        self.active_futures.remove(&listener_index);

        // Immediately start a new future for this listener
        let listener = self.listeners[listener_index].clone();
        let future = Box::pin(async move {
            let mut lock = listener.lock().await;
            lock.next().await
        });
        self.active_futures.insert(listener_index, future);

        Some((handler, interactive))
    }

    pub async fn stop(&mut self) -> Result<(), String> {
        self.active_futures.clear();
        self.started = false;

        // Call stop on all the listeners, and wait for all the stop
        // futures to complete before returning Ok(()) if all are Ok(())
        // or Err(e) if any are Err(e)
        let results = self
            .listeners
            .iter()
            .map(|listener| {
                let listener_clone = listener.clone();
                async move {
                    let mut lock = listener_clone.lock().await;
                    lock.stop().await
                }
            })
            .collect::<Vec<_>>();

        let results = futures::future::join_all(results).await;

        if results.iter().all(|r| r.is_ok()) {
            Ok(())
        } else {
            Err("Error stopping listeners".to_string())
        }
    }
}

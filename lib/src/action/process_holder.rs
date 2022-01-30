use std::sync::Arc;

use tokio::sync::RwLock;
use tracing::trace;

use crate::{command::Supervisor, error::RuntimeError, signal::process::SubSignal};

#[derive(Clone, Debug, Default)]
pub struct ProcessHolder(Arc<RwLock<Option<Supervisor>>>);
impl ProcessHolder {
	pub async fn is_running(&self) -> bool {
		self.0
			.read()
			.await
			.as_ref()
			.map(|p| p.is_running())
			.unwrap_or(false)
	}

	pub async fn is_some(&self) -> bool {
		self.0.read().await.is_some()
	}

	pub async fn drop_inner(&self) {
		trace!("dropping supervisor");
		self.0.write().await.take();
		trace!("dropped supervisor");
	}

	pub async fn replace(&self, new: Supervisor) {
		trace!("replacing supervisor");
		if let Some(_old) = self.0.write().await.replace(new) {
			trace!("replaced supervisor");
			// TODO: figure out what to do with old
		} else {
			trace!("not replaced: no supervisor");
		}
	}

	pub async fn signal(&self, sig: SubSignal) {
		if let Some(p) = self.0.read().await.as_ref() {
			trace!("signaling supervisor");
			p.signal(sig).await;
			trace!("signaled supervisor");
		} else {
			trace!("not signaling: no supervisor");
		}
	}

	pub async fn kill(&self) {
		if let Some(p) = self.0.read().await.as_ref() {
			trace!("killing supervisor");
			p.kill().await;
			trace!("killed supervisor");
		} else {
			trace!("not killing: no supervisor");
		}
	}

	pub async fn wait(&self) -> Result<(), RuntimeError> {
		if let Some(p) = self.0.read().await.as_ref() {
			trace!("waiting on supervisor");
			p.wait().await?;
			trace!("waited on supervisor");
		} else {
			trace!("not waiting: no supervisor");
		}

		Ok(())
	}
}

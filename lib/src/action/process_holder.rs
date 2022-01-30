use std::sync::Arc;

use tokio::sync::RwLock;

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
		self.0.write().await.take();
	}

	pub async fn replace(&self, new: Supervisor) {
		if let Some(_old) = self.0.write().await.replace(new) {
			// TODO: figure out what to do with old
		}
	}

	pub async fn signal(&self, sig: SubSignal) {
		if let Some(p) = self.0.read().await.as_ref() {
			p.signal(sig).await;
		}
	}

	pub async fn kill(&self) {
		if let Some(p) = self.0.read().await.as_ref() {
			p.kill().await;
		}
	}

	pub async fn wait(&self) -> Result<(), RuntimeError> {
		if let Some(p) = self.0.read().await.as_ref() {
			p.wait().await?;
		}
			Ok(())
	}
}

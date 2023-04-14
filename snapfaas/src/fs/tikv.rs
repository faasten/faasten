use std::sync::Arc;
use std::time::Instant;

use super::STAT;
use tikv_client::RawClient;

#[derive(Clone)]
pub struct TikvClient {
    tokio_runtime: Arc<tokio::runtime::Runtime>,
    client: RawClient,
}

impl TikvClient {
    pub const fn new(client: RawClient, tokio_runtime: Arc<tokio::runtime::Runtime>) -> Self {
        TikvClient {
            tokio_runtime,
            client,
        }
    }
}

impl super::BackingStore for TikvClient {
    fn get(&self, key: &[u8]) -> Option<Vec<u8>> {
        STAT.with(|stat| {
            let now = Instant::now();
            let res = self
                .tokio_runtime
                .block_on(async { self.client.get(Vec::from(key)).await.expect("tikv get") });
            stat.borrow_mut().get += now.elapsed();
            stat.borrow_mut().get_val_bytes += res.as_ref().map_or(0, |v| v.len());
            stat.borrow_mut().get_key_bytes += key.len();
            res
        })
    }

    fn put(&self, key: &[u8], value: &[u8]) {
        STAT.with(|stat| {
            let now = Instant::now();
            self.tokio_runtime.block_on(async {
                self.client
                    .put(Vec::from(key), value)
                    .await
                    .expect("tikv put")
            });
            stat.borrow_mut().put += now.elapsed();
            stat.borrow_mut().put_val_bytes += value.len();
            stat.borrow_mut().put_key_bytes += key.len();
        })
    }

    fn add(&self, key: &[u8], value: &[u8]) -> bool {
        STAT.with(|stat| {
            let now = Instant::now();
            let res = self.cas(key, None, value).is_ok();
            stat.borrow_mut().add += now.elapsed();
            stat.borrow_mut().add_val_bytes += value.len();
            stat.borrow_mut().add_key_bytes += key.len();
            res
        })
    }

    fn cas(
        &self,
        key: &[u8],
        expected: Option<&[u8]>,
        value: &[u8],
    ) -> Result<(), Option<Vec<u8>>> {
        STAT.with(|stat| {
            let now = Instant::now();
            let (orig, success) = self.tokio_runtime.block_on(async {
                self.client
                    .with_atomic_for_cas()
                    .compare_and_swap(Vec::from(key), expected.map(Vec::from), value)
                    .await
                    .expect("tikv cas")
            });
            stat.borrow_mut().cas += now.elapsed();
            stat.borrow_mut().cas_val_bytes += value.len();
            stat.borrow_mut().cas_key_bytes += key.len();
            if success {
                Ok(())
            } else {
                Err(orig)
            }
        })
    }

    fn del(&self, key: &[u8]) {
        self.tokio_runtime
            .block_on(async { self.client.delete(Vec::from(key)).await.expect("tikv del") });
    }

    fn get_keys(&self) -> Option<Vec<&[u8]>> {
        todo!()
    }
}

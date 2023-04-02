use tikv_client::RawClient;

pub struct TikvClient {
    tokio_runtime: tokio::runtime::Runtime,
    client: RawClient,
}

impl TikvClient {
    pub const fn new(client: RawClient, tokio_runtime: tokio::runtime::Runtime) -> Self {
        TikvClient { tokio_runtime, client }
    }
}

impl super::BackingStore  for TikvClient {
    fn get(&self, key: &[u8]) -> Option<Vec<u8>> {
        self.tokio_runtime.block_on(async {
            self.client.get(Vec::from(key)).await.expect("tikv get")
        })
    }

    fn put(&self, key: &[u8], value: &[u8]) {
        self.tokio_runtime.block_on(async {
            self.client.put(Vec::from(key), value).await.expect("tikv put")
        })
    }

    fn add(&self, key: &[u8], value: &[u8]) -> bool {
        self.cas(key, None, value).is_ok()
    }

    fn cas(&self, key: &[u8], expected: Option<&[u8]>, value: &[u8]) -> Result<(), Option<Vec<u8>>> {
        let (orig, success) = self.tokio_runtime.block_on(async {
            self.client.with_atomic_for_cas().compare_and_swap(Vec::from(key), expected.map(Vec::from), value).await.expect("tikv cas")
        });
        if success {
            Ok(())
        } else {
            Err(orig)
        }
    }

    fn del(&self, key: &[u8]) {
        self.tokio_runtime.block_on(async {
            self.client.delete(Vec::from(key)).await.expect("tikv del")
        });
    }

    fn get_keys(&self) -> Option<Vec<&[u8]>> {
        todo!()
    }
}

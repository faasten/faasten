scheduler: scheduler --listen 0.0.0.0:1234 -q 10
webfront: webfront --listen 0.0.0.0:8080 --base-url http://localhost:8080 --faasten-scheduler 127.0.0.1:1234 --lmdb /tmp/faasten-db --secret-key frontends/webfront/secret.pem --public-key frontends/webfront/public.pem
multivm: multivm --scheduler 127.0.0.1:1234 --memory 4096 --lmdb /tmp/faasten-db

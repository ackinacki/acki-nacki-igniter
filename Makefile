build:
	docker build -t gossip-igniter-local --progress=plain .

restart: build
	docker compose kill
	docker compose up -d

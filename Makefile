include .env

PROJECT_DIR=$(abspath $(CURDIR))

run:
	RUST_LOG=info cargo run ./config.yml
docs:
	cargo doc --no-deps
docs-open:
	cargo doc --no-deps --open

.PHONY: db_tables_load
db_tables_load:
	echo $(PROJECT_DIR)
	docker run --rm -ti --net=host -v $(PROJECT_DIR):/src postgres:13.3 psql -f /src/tables.sql -U $(POSTGRES_USER) -h $(POSTGRES_HOST) -d $(POSTGRES_DB)

.PHONY: db_open
db_open:
	docker run --rm -ti --net=host postgres:13.3 psql -U $(POSTGRES_USER) -h $(POSTGRES_HOST) -d $(POSTGRES_DB)

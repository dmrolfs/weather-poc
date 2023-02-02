# weather-poc


Assuming you have Docker CLI installed, you can initialize the database via:
```shell
$ ./scripts/init_db.sh
```

or if you have [justfile](https://just.systems/man/en/chapter_1.html) installed:
```shell
$ just init-db
```

Assuming you have the [rust toolchain](https://www.rust-lang.org/tools/install) installed, you can build and run the 
server via:

```shell
$ RUST_LOG="info, weather=debug" cargo run -- --secrets ./resources/secrets.yaml
```

or via justfile, if you also have `bunyan` installed (via `cargo install bunyan`) :

```shell
$ just r
```

View Swagger-ui at `localhost:8000/swagger-ui`

Use `Forecast` zones in the system; e.g., 
- `WAZ558` - Seattle and Vicinity
- `ILZ045` - Champaign county IL`
- `KYZ069` - somewhere in Kentucky
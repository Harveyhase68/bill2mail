@echo off
cls

If "%1%"=="x" (
	echo Clean
 	cargo clean
)

echo Release
cargo build --release

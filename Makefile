.PHONY: clean
clean:
	cargo clean
	rm -f Cargo.lock
	rm -f *.json
	rm -f *.png
	rm -rf out*

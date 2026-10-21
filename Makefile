.PHONY: test test-bdd

test:
	bash -n ticket
	python3 -m unittest discover -s tests -v

test-bdd:
	python3 -m behave

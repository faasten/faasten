FUNCTIONS=hello fsutil
OUTPUTS=$(patsubst %, output/%.img, $(FUNCTIONS))

.PHONY: all clean
all: $(OUTPUTS)

output/%.img: ./%/*
	@truncate -s 500M $@
	@mkfs.ext2 -F $@
	@ \
		if [ -f ./$*/Makefile ]; then \
			make -C ./$*; \
			cptofs -t ext2 -i $@ ./$*/out/* /; \
		else \
			cptofs -t ext2 -i $@ ./$*/* /; \
		fi
	@e2fsck -f $@
	@resize2fs -M $@

clean:
	rm output/*

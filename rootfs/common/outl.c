#include <stdlib.h>
#include <stdio.h>
#include <stdint.h>
#include <sys/io.h>
int main(int argc, char * argv[]) {
	int magic = strtoul(argv[1], NULL, 0);
	int port = strtoul(argv[2], NULL, 0);
  if (iopl(3)) {perror("iopl"); exit(1);}
  outl(magic, port);
}

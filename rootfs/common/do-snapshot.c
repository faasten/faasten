#include <stdlib.h>
#include <stdio.h>
#include <stdint.h>
#include <sys/io.h>
#include <sys/mount.h>
#include <time.h>
int main(int argc, char * argv[]) {
  int val = strtol(argv[1], NULL, 10);
  if (iopl(3)) {perror("iopl"); exit(1);}
  outl(124, 0x3f0);
  outl(val, 0x3f0);
}

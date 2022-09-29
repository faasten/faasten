#include <stdio.h>
#include <sys/ioctl.h>
#include <linux/random.h>
#include <fcntl.h>

int main() {
  int rand = open("/dev/random", 0);
  int amount = 10241024;
  if (ioctl(rand, RNDADDTOENTCNT, &amount) < 0) {
    perror("ioctl");
  }
}


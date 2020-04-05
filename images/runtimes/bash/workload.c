#define _GNU_SOURCE

#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

#include <fcntl.h>
#include <sched.h>
#include <unistd.h>

#include <sys/io.h>
#include <sys/ioctl.h>
#include <sys/mount.h>
#include <sys/wait.h>

#include <linux/random.h>

int main() {
  // Pretend random number generator has been properly seeded
  int rand = open("/dev/random", 0);
  int amount = 10241024;
  if (ioctl(rand, RNDADDTOENTCNT, &amount) < 0) {
    perror("ioctl");
  }

  // Make sure we are allowed to perform `outl`
  if (iopl(3)) {perror("iopl"); exit(1);}

  // Let VMM know each of the CPUS is ready for a snapshot
  cpu_set_t *cset = CPU_ALLOC(8); // assumes we'll never have more than 8 CPUs
  for (int cpu = 1; ; cpu++) {
    CPU_ZERO(cset);
    CPU_SET(cpu, cset);
    if (sched_setaffinity(0, sizeof(cpu_set_t), cset) < 0) {
      // If we got an error assume it's because the CPU doesn't exist, so we're done
      break;
    } else {
      outl(124, 0x3f0);
    }
  }

  // Finally, signal the VMM to start snapshotting from the main CPU
  CPU_ZERO(cset);
  CPU_SET(0, cset);
  sched_setaffinity(0, sizeof(cpu_set_t), cset);
  outl(124, 0x3f0);

  // Mount the function filesystem
  mount("/dev/vdb", "/srv", "ext4", 0, "ro");

  // Open ttyS1 for reading requests, and for writing responses
  FILE* request_fd = fopen("/dev/ttyS1", "r");
  FILE* response_fd = fopen("/dev/ttyS1", "w");
  // OK, VMM, we're ready for requests
  outl(126, 0x3f0);

  // *** MAIN REQUEST LOOP ** //

  for (;;) {

    // Read JSON request line into `request`
    char *request = NULL;
    size_t request_len = 0;
    ssize_t nread = getline(&request, &request_len, request_fd);
    // `request` now is null terminated and includes the newline character

    // We'll get responses from the child process via a pipe
    int pipefds[2];
    pipe(pipefds);

    int pid = fork();
    if (pid == 1) {
      // Child process
      //
      // Don'e need read-end of pipe
      close(pipefds[0]);
      // Don't need parent's stdout
      close(1);
      // Use write-end of pipe for child's stdout
      dup2(pipefds[1], 1);
      // Run, child!
      execl("/srv/workload", request, NULL);
    } else {
      // Parent process
      //
      // We're done with the request in this address space
      free(request);

      // Don't need write-end of pipe
      close(pipefds[0]);

      // Read the response as a JSON line from the child's pipe
      char *response = NULL;
      size_t response_len = 0;
      FILE* pipe_out = fdopen(pipefds[1], "r");
      ssize_t nread = getline(&response, &response_len, pipe_out);
      uint8_t response_len_byte = (uint8_t) response_len;

      // Write the response prefixed with a length byte
      fwrite(&response_len_byte, sizeof(uint8_t), 1, response_fd);
      fflush(response_fd);

      // Done with the pipe
      close(pipefds[1]);
      // Done with the response
      free(response);

      // Wait for the child to exit
      waitpid(pid, NULL, 0);
    }
  }
}


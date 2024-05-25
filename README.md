*TL;DR Faasten is an architecture for securing cloud applications that moves
application code out of the trusted computing base (TCB) by offering a
decentralized information flow control (DIFC) interface for the FaaS
programming model.*

# What is Faasten?

Faasten is an architecture for securing cloud applications. The architecture
looks pretty much like the existing ones but distinguishes itself in the system
interface. The high bit is that Faasten builds policy enforcement into its
system interface---a set of CloudCalls, and developers no longer have to trust
that their code properly enforces intended high-level policies. Take the
example of a photo management application, developers declare a user's photo
data is private to the owner user and Faasten enforces the policy throughout
the application. 

Moreover, the interface is self-encapsulating. It captures not only regular
data and their policies like photo data in the example above but also functions
themselves and their privileges and meta-data for data discovery.

## Programming Model

For the programming model, Faasten assumes the Function-as-a-Service model.
Applications are composed of functions whose execution is request-driven.  User
clients (e.g., the [fstn](https://github.com/faasten/fstn) CLI tool) can emit
requests which arrive at the system gateway and are then forwarded to the
system scheduler. Functions themselves can generate requests that invoke
another function as well.

Faasten functions run in Firecracker microVMs. Each microVM instance boots to a
Faasten wrapped runtime that loads a function and exposes the Faasten interface
that is the only channel for functions to communicate sensitive data with the
external world.

This Faasten prototype launches a Firecracker mciroVM through a wrapper binary
[firerunner](./snapfaas/bins/firerunner). (Note: unfortunately, the binary is
written for a very old revision of Firecracker.)

The folder [rootfs](./rootfs) contains the tools for building a root filesystem
image that contains a wrapped runtime. Currently, only Python3 is supported.

## Security Reference Monitor

The security reference monitor is what distinguishes a Faasten system from any
other FaaS/cloud systems. It defines and implements the aforementioned Faasten
interface.

Faasten adopts the security paradigm called *information flow control* that
controls information flows instead of discrete accesses. The control is
possible because security policies, also often known as IFC labels, are
constructed to form a lattice. Following such a modelling, a secure information
flow is defined as one that flows from low to high in the lattice.
So far, we've described the modelling. To actually does the control, we need to
define an interface the sits between computation and data and embed information
flow checks in its composing methods.

The Faasten label model Buckle is [here](https://github.com/alevy/labeled). It
is a decentralized label model. Buckle labels are Boolean formulas of
principals (e.g., alice), and the label lattice is constructed in a
decentralized way by the logical implication. More importantly, Buckle features
creation of new principals by delegation (e.g., alice can create new principal
alice:photo-management). That is, Buckle principal futher encodes a principal
hierarchy.

The interface consists of a set of object types and their CloudCalls that
create, modify, read them. They are defined in
[snapfaas/src/fs/mod.rs](./snapfaas/src/fs/mod.rs) and
[snapfaas/src/blobstore/mod.rs](./snapfaas/src/blobstore/mod.rs).

The security reference monitor is a single-threaded loop serving CloudCalls
made by functions, defined in
[snapfaas/src/syscall\_server.rs](./snapfaas/src/syscall_server.rs)

## The Rest Non-Faasten Specific System Components

This respository also includes in-house implementations of the rest necessary
system components for us to deploy the system. They are non-Faasten specific.

A Faasten system must be able to assign principals to remote users. The
assignment happens after successful authentication at the system gateway. Our
implementation is in [webfront](./frontends/webfront). 

A Faasten system must be able to schedule requests to a cluster of worker machines.
Our implementation includes a simple scheduler in [scheduler](./snapfaas/bins/scheduler)
and a local manager running on each worker machine that talks to the scheduler
in [multivm](./snapfaas/bins/multivm).

# Development tool

We include a development tool [singlevm](./snapfaas/bins/singlevm). It boots
one single VM, reads line-delimited requests from the stdin, and proxies
requests and responses.

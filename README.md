*** CONCEPT ***

# Landlord
Landlord provides the ability to run multiple JVM based applications on the one JVM. This is otherwise known as "multi tenancy", hence the name, "Landlord". :-)

> There are similar projects out there, including [Nailgun](https://github.com/martylamb/nailgun#nailgun). My requirements are to address security from the beginning and focus on using the mininum number of threads by avoiding blocking IO. Each thread's stack space will take up 1MiB of memory. I'm also looking for isolation and want to consider cgroups. While projects like Nailgun could perhaps be updated to manage my requirements, I feel that a clean start is appropriate. Retrofitting non-blocking/asynchronous behaviour in particular is difficult. Another goal is that I'd like there to be a very high degree of compatibility between running a JVM program normally vs Landlord. Hence the CLI tool it uses presents nearly all of the same arguments as the `java` command. A high degree of compatibility equates to wider adoption.

## Why
JVM programs take up too much resident memory. Back in the day of Java 1.1, a minimal JVM application outputting "Hello world" would take about about 4MiB of resident memory. Nowadays, the same program in Java 8 takes around 45MiB of resident memory i.e. over 10 times as much! While Java 9's modules may help reduce the JVM's footprint, there's a lot of commonality between the JRE's of an individual JVM that can be shared.

Also, compare a typical JVM "microservice" to one written using a native target, such as LLVM; the JVM one will occupy around 350MiB which is more than 10 times the amount of memory when compared to a native one. The JVM's consumption of memory may have been fine for the monolith, but when it comes to running many JVM based microservices (processes), their resident memory usage makes you want to program in something closer to the metal... or seek the "landlord" project!

Discounting the regular JVM overhead of runnning the first service, Running Landlord will reduce a typical "hello world" down to the requirements of its classpath - typically less than 1KiB (yes, you read that right!).

## What
Landlord is a daemon service named `landlordd`. `landlordd` launches the JVM and runs some code that provides a secure RESTful HTTP service where you can submit your JVM program to run. You may also send various [POSIX signals](https://en.wikipedia.org/wiki/Signal_(IPC)) that are trapped by your program in the conventional way for the JVM. You manage the daemon's lifecycle as per other services on your machines e.g. via initd. 

A client is also provided and named `landlord`. This client interfaces with `landlordd` and accepts a filesystem via stdin.

## How
From a code perspective, you must also replace the use of the JDK's `System` object with a one provided by Landlord. That's about it!

From the command line, simply use the `landlord` command instead of the `java` tool and pass its filesystem via tar on `stdin` and you're good to go.

There are some cavaets in that some of the `java` tool's options are not supported i.e. `-X`, `-agentlib` and others associated with the JVM as a whole.

Under the hood, `landlord` will perform a secure HTTPS `POST` of its arguments, including the streaming of the `tar` based file system from `stdin`. `landlordd` will consume the `POST` and create a new class loader to load your jars and class files from the filesystem your provided, and in accordance with `cp` and `jar` arguments. Upon your program exiting, the classloader will available for garbarge collection.

### An example

The obligatory "hello world":

```java
import landlord.System;

public class Main {

    public static void main(String[] args) {
        System.out.println("Hello World!");
    }
}
```

> Note the import of `landlord.System`. This is important in order to obtain a system in relation to your particular program. If you use the regular JDK system then you'll get the system of the JVM as a whole.

Upon compiling and supposing a folder containing our "hello world" class at `./hello-world/out/production/hello-world`:

```
tar -c . | landlord -cp ./hello-world/out/production/hello-world Main
```

## landlord
`landlord` will `POST` your commands to `landlordd` and then wait on a response. The response will yield the exit code from your program which will then cause `landlord` exit with the same response code.

Any POSIX signals sent to `landlord` while it is waiting for a reply will be forwarded onto `landlordd` and received by your program.

Note that in the case of long-lived programs (the most typical scenario for a microservice at least), then `landlord` will not return until your program terminates.

## landlordd
You can run as many `landlordd` daemons as you wish and as your system will allow. When starting `landlordd`, you typically provide a secret for clients to connect with, and an IP and port for landlordd to listen for HTTPS requests on. Any memory requirements for the JVM should consider all of the programs that it will host.

(c)opyright 2017, Christopher Hunt

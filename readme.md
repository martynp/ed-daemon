# ED Daemon

Ed is a simple docker runtime manager for pushing container images to remote systems - the intention is to provide a simple mechanism to deploy containers which does not rely on maintaing centralised registries.

This is the daemon which runs on the remote system.

The daamon manages the lifecycle of deployed containers using the docker API and docker cli. Access to the daemon is via a REST interface which is secured using mTLS.

## Usage

The daemon will execute a docker cli run command such as:

``` bash
docker run -d -it -p 80:8000 -v /var/data/app:/usr/share/nginx/html --name website nginx
```

The `-d -it` flags are required and the `--name` parameter is set by the daemon, the remainder of the parameters are set in the ed-daemon configuration file `/etc/edd/config.json`.

``` json
{
    "deployments": [
        {
            "name": "website",
            "args": ["-p", "80:8000", "-v", "/var/data/app:/usr/share/nginx/html"]
        }
    ]
}
```

The container can then be controlled using:

- `/v1/deployments/website/load`
- `/v1/deployments/website/stop`
- `/v1/deployments/website/start`

The `load` operation accepts a `.tar` or `.tar.gz` upload, and will load the new image, stop any existing website container and then re-tag and start the new container.

The `stop` and `start` operations allow control over a running or stopped container.

## Example

In this example an nginx container image is generated containing the files for a static website, the created image is pushed to the remote endpoint.

`Dockerfile`:

``` Dockerfile
FROM nginx:latest

COPY ./website/ /usr/share/nginx/html/
```

``` bash
docker build . -t website:latest
curl --cacert ca.crt \
     --key client.key \
     --cert client.crt \
     -X POST -H "Content-Type:application/x-tar" -T - 'https://192.168.0.100:8866/v1/website/load'
```

## Installation

Installation is easiest using the rust cargo manager, rust must be installed to a user which has permission to use docker - this can be done using the instructions at https://www.rust-lang.org/tools/install.

The ed-daemon is then installed using:

```
cargo install ed-daemon
```

This will install the executable to `/home/[user]/.cargo/bin/ed-daemon`.
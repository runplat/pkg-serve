# pkg-serve

This project is for an ephemeral file server for use with daemonset scripts executing on K8's cluster nodes.

It allows for storing scripts as a ConfigMap, deploying this image as a Daemonset, connecting to a backing storage service,
executing scripts via `nsenter` on the Node, and then providing an easy file server for installing various packages.

Currently, this project is focused on the Azure ecosystem to use with:

- AKS
- ACR
- Azure Storage Blobs

However, it should be fairly straightforward to adopt to other clouds, as all of the code is fairly short.

## Getting Started

To get started, first you can use the provided `tasks.yaml` to build and push the pkg-serve image to your own ACR registry. By creating a task, you can keep pkg-serve
image up to date with a source-trigger, or even just run it on demand with ACR Task's quick build feature.

Next create a `pkg` container in the storage account of your choice, upload some blobs and create `packages.toml` file to configure the routes to serve.

Finally, ensure you've assigned a role to your AKS Nodepool's kubelet identity to access both ACR and the Storage Account of your choice.

Last but not least, create your config map and daemonset from the examples below, and run `kubectl apply` to execute the daemonset.

## Configuring the pkg server

All configuration is done within the storage account to re-use the provided tooling. When the server starts it will look for a container called `pkg`. And a blob called `packages.toml`.

`packages.toml` lists available packages that can be served by the server by configuring the name of the package and a "tag" value.

Here is an example file,

```toml
[mypackage.latest]
path = 'mypackage-123456.deb'
```

The first table `mypackage` will be the name of the package being served. This table is a map of "tags" that each map to a blob path within the same container.
For example, `latest` would be the tag name and `mypackage-123456.deb` is the name of the blob in the same container.

Putting it all together, the path to download this blob during runtime would be `/pkg/mypackage/latest`. You can imagine building automation that updates this file
within the build pipeline, or you could just manually maintain it from your own machine.

## Example Usage

Suppose you have a project that produces debian packages which you wish to test before uploading that package to a central public
repository. Typically, you could stick this package in an Azure Storage account, make it anonymous, and then download that package
onto your node during your test.

The problem is that, now you've enabled anonymous access to one of your storage accounts. `pkg-serve` solves this problem by replacing
the anonymous source with a local source which authenticates with your storage account using the node's identity. This avoids having to
put additional authn logic in install scripts which would increase the overhead and complexity of the install script.

The first part of the story is the script that you wish to run on your K8's node. Here is an example ConfigMap:

```yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: pkgserve-actions-example
  labels:
    app: pkgserve-example
data:
  install: |
    #!/usr/bin/env bash
    set -xe

    if [[ -f /opt/sentinel ]]; then
       if [[ ! -z "\$1" ]]; then
         echo "received positional argument, forcing cleanup"
         rm /opt/sentinel || true
       else
         echo "Already ran, exiting"
         exit 0
       fi
    fi

    # Start installing packages on the node
    pushd /tmp || exit 1
    pkg_name="mypackage"
    pkg_tag="latest"
    pkg_output="./mypackage.deb"

    # This port is set by the pkg-serve container when the DaemonSet starts
    port="$PKG_SERVE_RUN_PORT"
    pkg_source="localhost:$port/pkg/$pkg_name/$pkg_name"

    # Fetch the package like you would any other anonymous package, except this request will never leave the network
    echo "Getting package from source -- $pkg_source"
    wget -O "$pkg_name" "$pkg_source"

    # Install the package on the node
    apt-get install $pkg_output -y
    popd

    # Run any additional setup scripts here

    touch /opt/sentinel
```

The key entrypoint in the above script is the line with, `port="$PKG_SERVE_RUN_PORT"`. This environment variable will be set by the `entrypoint.sh` script
in this repo. `entrypoint.sh` will execute when the DaemonSet container starts. When combined with the line `pkg_source="localhost:$port/pkg/$pkg_name/$pkg_name"` you can
create a url to the package you need which can simply be downloaded with `wget -O "$pkg_name" "$pkg_source"`.

Here is an example DaemonSet:

```yaml
apiVersion: apps/v1
kind: DaemonSet
metadata:
  name: &name pkgserve-example
  labels:
    app: *name
spec:
  selector:
    matchLabels:
      app: *name
  template:
    metadata:
      labels:
        app: *name
    spec:
      hostNetwork: true
      hostPID: true
      containers:
      - image: myregistry.azurecr.io/pkg-serve:latest # Build and push your own version of this image w/ the provided tasks.yaml file
        imagePullPolicy: Always
        name: *name
        args: [ "install", "mystorageaccountname" ]
        resources:
          requests:
            cpu: 0.5
            memory: 1000Mi
          limits:
            cpu: 0.5
            memory: 1000Mi
        securityContext:
          privileged: true
        volumeMounts:
        - name: actions
          mountPath: "/opt/actions"
        - name: hostmount
          mountPath: "/mnt/actions"
      volumes:
      - name: hostmount
        hostPath:
          path: /opt/actions
          type: DirectoryOrCreate
      - name: actions
        configMap:
          name: pkgserve-actions-example
```

The key entrypoint in the above DaemonSet definition is the line with `args: [ "install", "mystorageaccountname" ]`. The first argument `install` is the name of the config key
in the ConfigMap. The second argument `mystorageaccountname`, is the name of the Azure Storage Account the Node's identity that has an assigned blob storage role.

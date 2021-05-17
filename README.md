
> Note: This has not be deployed on crates.io. Install from git or see below for binary install only. 

**Under development**

Simple command line for recursive `npm audit` reporting. Run `npm install` and `npm audit` to build a .tar.gz report.  of all sub folders that contain a package.json to build a audit `.tar.gz` with the combined `package-lock.json` files and the summary of the `npm audit`

## Install

With **cargo install**

Binary install via [binst](https://github.com/jeremychone/rust-binst)

```sh
binst install naudit -r https://binst.io/jc-repo
```
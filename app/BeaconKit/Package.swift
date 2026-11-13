// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "BeaconKit",
    platforms: [.iOS(.v18)],
    products: [.library(name: "BeaconKit", targets: ["BeaconKit"])],
    targets: [
        .target(name: "BeaconKit"),
        .testTarget(name: "BeaconKitTests", dependencies: ["BeaconKit"]),
    ]
)

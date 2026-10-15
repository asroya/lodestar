import SwiftUI
@main struct HelloApp: App {
    var body: some Scene {
        WindowGroup {
            VStack(spacing: 24) {
                Text("✦").font(.system(size: 120))
                Text("Sideloaded with Lodestar").font(.title)
            }
        }
    }
}

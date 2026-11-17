import BeaconKit
import Foundation
import Security

/// The config header value, the app's only secret (ADR D6). The widget never reads it.
struct Keychain: HeaderStore {
    private var item: [String: Any] {
        [kSecClass as String: kSecClassGenericPassword, kSecAttrService as String: "dev.jirathip.beacon",
         kSecAttrAccount as String: "config-header"]
    }

    var header: String? {
        var out: AnyObject?
        let query = item.merging([kSecReturnData as String: true]) { a, _ in a }
        guard SecItemCopyMatching(query as CFDictionary, &out) == errSecSuccess, let data = out as? Data else {
            return nil
        }
        return String(decoding: data, as: UTF8.self)
    }

    func setHeader(_ value: String) throws {
        SecItemDelete(item as CFDictionary)
        guard !value.isEmpty else { return }
        let add = item.merging([
            kSecAttrAccessible as String: kSecAttrAccessibleAfterFirstUnlock, kSecValueData as String: Data(value.utf8),
        ]) { a, _ in a }
        let status = SecItemAdd(add as CFDictionary, nil)
        guard status == errSecSuccess else { throw BeaconError("keychain \(status)") }
    }
}

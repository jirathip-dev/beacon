import Foundation
import Security

/// The config header value, the app's only secret (ADR D6). The widget never reads it.
enum Keychain {
    private static var item: [String: Any] {
        [kSecClass as String: kSecClassGenericPassword, kSecAttrService as String: "dev.jirathip.beacon",
         kSecAttrAccount as String: "config-header"]
    }

    static var header: String? {
        var out: AnyObject?
        let query = item.merging([kSecReturnData as String: true]) { a, _ in a }
        guard SecItemCopyMatching(query as CFDictionary, &out) == errSecSuccess, let data = out as? Data else {
            return nil
        }
        return String(decoding: data, as: UTF8.self)
    }

    static func setHeader(_ value: String) {
        SecItemDelete(item as CFDictionary)
        guard !value.isEmpty else { return }
        let add = item.merging([
            kSecAttrAccessible as String: kSecAttrAccessibleAfterFirstUnlock, kSecValueData as String: Data(value.utf8),
        ]) { a, _ in a }
        SecItemAdd(add as CFDictionary, nil)
    }
}

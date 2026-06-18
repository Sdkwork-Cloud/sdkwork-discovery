use sdkwork_discovery_contract::{
    ConfigEncryptor, EncryptedValue, EncryptionAlgorithm, NoopConfigEncryptor,
};

#[test]
fn encrypted_value_round_trips_through_storage_string() {
    let value = EncryptedValue {
        algorithm: EncryptionAlgorithm::Aes256Gcm,
        nonce: vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12],
        ciphertext: b"secret-config-body".to_vec(),
    };

    let encoded = value.to_storage_string();
    assert!(EncryptedValue::is_encrypted(&encoded));

    let decoded = EncryptedValue::from_storage_string(&encoded).expect("valid storage string");
    assert_eq!(decoded, value);
}

#[test]
fn noop_encryptor_is_disabled_and_round_trips_plaintext() {
    let encryptor = NoopConfigEncryptor;
    assert!(!encryptor.is_enabled());

    let plaintext = b"config-value";
    let encrypted = encryptor.encrypt(plaintext).expect("encrypt");
    let decrypted = encryptor.decrypt(&encrypted).expect("decrypt");
    assert_eq!(decrypted, plaintext);
}

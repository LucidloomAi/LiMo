// Checks the release key that LIMO_RELEASE_KEY_FILE names, before a build
// signs with it. Run by scripts/signing.mjs before Gradle starts, and by
// scripts/release-signing.gradle before Gradle builds a release:
//
//   java scripts/ReleaseKeyCheck.java
//
// (Java 11 and later run one source file as it is; the build needs JDK 17.)
//
// LIMO_RELEASE_KEY_FILE is the full path of a properties file, outside every
// repository, with four keys: storeFile (the keystore: a full path, or a path
// from the properties file's own folder), storePassword, keyAlias and
// keyPassword. This reads the variable and the file itself, so no password is
// ever on a command line or in another process's environment. It prints one
// line and nothing else:
//
//   OK<tab><the certificate's SHA-256, lower case hex><tab><its subject>
//   FAIL<tab><what is wrong, in plain words>
//
// and exits 0 for OK, 1 for FAIL. Neither line ever holds a password, a path,
// a value from the properties file or the exception Java threw: a build's log
// may be pasted anywhere. A FAIL names which of the four keys is wrong, and
// nothing more of it.
//
// The file is read as UTF-8 with java.util.Properties' rules, exactly as
// scripts/release-signing.gradle reads it, and storeFile is found the same way
// there, so the key checked here is the key Gradle signs with.

import java.io.File;
import java.io.FileInputStream;
import java.io.IOException;
import java.io.InputStream;
import java.io.InputStreamReader;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.security.Key;
import java.security.KeyStore;
import java.security.KeyStoreException;
import java.security.MessageDigest;
import java.security.PrivateKey;
import java.security.UnrecoverableKeyException;
import java.security.cert.Certificate;
import java.security.cert.X509Certificate;
import java.util.ArrayList;
import java.util.List;
import java.util.Properties;
import java.util.regex.Pattern;

public class ReleaseKeyCheck {
  static final String VARIABLE = "LIMO_RELEASE_KEY_FILE";
  static final String[] KEYS = {"storeFile", "storePassword", "keyAlias", "keyPassword"};
  static final Pattern DEBUG_SUBJECT = Pattern.compile("(^|,\\s*)CN=Android Debug(,|$)");

  public static void main(String[] args) {
    String line;
    try {
      line = check(System.getenv(VARIABLE));
    } catch (Throwable t) {
      // Only the kind of failure: an exception's message can hold a path.
      line = fail("The release key could not be checked (" + t.getClass().getSimpleName() + ").");
    }
    System.out.println(line);
    System.exit(line.startsWith("OK\t") ? 0 : 1);
  }

  static String fail(String message) {
    return "FAIL\t" + message;
  }

  static String check(String variable) throws Exception {
    if (variable == null || variable.isEmpty()) {
      return fail(VARIABLE + " is not set.");
    }
    File propertiesFile = new File(variable);
    if (!propertiesFile.isAbsolute()) {
      return fail(VARIABLE + " must be a full path, such as C:\\keys\\limo-release.properties.");
    }
    if (!propertiesFile.isFile()) {
      return fail(VARIABLE + " points at no file. Set it to the full path of the release key's "
          + "properties file, or leave it unset for a build signed with Android's debug key.");
    }
    Properties p = new Properties();
    String raw;
    try {
      raw = new String(Files.readAllBytes(propertiesFile.toPath()), StandardCharsets.UTF_8);
      try (InputStream in = new FileInputStream(propertiesFile)) {
        p.load(new InputStreamReader(in, StandardCharsets.UTF_8));
      }
    } catch (IOException e) {
      return fail("The file " + VARIABLE + " points at cannot be read.");
    }

    List<String> missing = new ArrayList<>();
    for (String key : KEYS) {
      String value = p.getProperty(key);
      if (value == null || value.trim().isEmpty()) missing.add(key);
    }
    if (!missing.isEmpty()) {
      return fail("The file " + VARIABLE + " points at has no " + String.join(" and no ", missing)
          + ". It needs all four: storeFile, storePassword, keyAlias, keyPassword.");
    }
    String storePassword = p.getProperty("storePassword");
    String keyAlias = p.getProperty("keyAlias");
    String keyPassword = p.getProperty("keyPassword");

    File store = storeFile(propertiesFile, p.getProperty("storeFile"));
    if (!store.exists()) {
      String hint = backslashesLost(raw)
          ? " A backslash in a properties file starts an escape and is dropped: write the path with "
              + "forward slashes, such as C:/keys/limo-release.p12, or put the keystore beside the "
              + "properties file and give its name alone."
          : " A storeFile that is not a full path is read from the folder the properties file is in.";
      return fail("The keystore that storeFile names does not exist." + hint);
    }
    if (!store.isFile()) {
      return fail("storeFile names a folder, not a keystore.");
    }

    KeyStore ks;
    try {
      ks = KeyStore.getInstance(store, storePassword.toCharArray());
    } catch (KeyStoreException e) {
      return fail("The file storeFile names is not a keystore Java can read (PKCS12 or JKS).");
    } catch (IOException e) {
      if (e.getCause() instanceof UnrecoverableKeyException) {
        return fail("The keystore that storeFile names does not open with storePassword."
            + endsWithSpace("storePassword", storePassword));
      }
      return fail("The keystore that storeFile names cannot be read: storePassword is wrong, "
          + "or the file is damaged." + endsWithSpace("storePassword", storePassword));
    }

    if (!ks.containsAlias(keyAlias)) {
      return fail("The keystore holds no entry named by keyAlias." + endsWithSpace("keyAlias", keyAlias));
    }
    if (!ks.isKeyEntry(keyAlias)) {
      return fail("The entry keyAlias names holds a certificate only, not a key to sign with.");
    }
    Key key;
    try {
      key = ks.getKey(keyAlias, keyPassword.toCharArray());
    } catch (UnrecoverableKeyException e) {
      return fail("keyPassword does not open the key that keyAlias names. In a PKCS12 keystore, "
          + "which keytool makes unless told otherwise, the key's password is the keystore's password."
          + endsWithSpace("keyPassword", keyPassword));
    }
    if (!(key instanceof PrivateKey)) {
      return fail("The entry keyAlias names holds no private key to sign with.");
    }
    Certificate cert = ks.getCertificate(keyAlias);
    if (!(cert instanceof X509Certificate)) {
      return fail("The entry keyAlias names has no X.509 certificate.");
    }
    String subject = ((X509Certificate) cert).getSubjectX500Principal().toString();
    if (DEBUG_SUBJECT.matcher(subject).find()) {
      return fail("The key keyAlias names is an Android debug key (CN=Android Debug). "
          + "A release key must be one of the maker's own.");
    }
    byte[] digest = MessageDigest.getInstance("SHA-256").digest(cert.getEncoded());
    StringBuilder hex = new StringBuilder();
    for (byte b : digest) hex.append(String.format("%02x", b));
    return "OK\t" + hex + "\t" + subject.replace('\t', ' ').replace('\n', ' ');
  }

  // A full path, or a path from the properties file's own folder.
  static File storeFile(File propertiesFile, String value) {
    File f = new File(value);
    return f.isAbsolute() ? f : new File(propertiesFile.getAbsoluteFile().getParentFile(), value);
  }

  // A storeFile line with a single backslash in it: java.util.Properties
  // drops it, so C:\keys\x.p12 is read as C:keysx.p12.
  static boolean backslashesLost(String raw) {
    for (String line : raw.split("\\R")) {
      String t = line.trim();
      if (t.startsWith("storeFile") && t.replace("\\\\", "").contains("\\")) return true;
    }
    return false;
  }

  // A value that ends in a space or a tab keeps it: a common reason for a
  // password that looks right and does not open anything.
  static String endsWithSpace(String key, String value) {
    return (value.endsWith(" ") || value.endsWith("\t"))
        ? " (" + key + " ends with a space, which counts as part of it.)"
        : "";
  }
}

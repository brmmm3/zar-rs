package org.zar.jarlib.jni;

public final class ZarException extends Exception {
    public ZarException(String message) {
        super(message);
    }

    public ZarException(String message, Throwable cause) {
        super(message, cause);
    }
}

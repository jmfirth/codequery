function formatName(first, last) {
    return first + " " + last;
}

class Logger {
    constructor(prefix) {
        this.prefix = prefix;
    }

    log(message) {
        console.log(this.prefix + ": " + message);
    }
}

const double = (x) => x * 2;

export function exported() {
    return true;
}

export class ExportedLogger {
    info(msg) {
        console.info(msg);
    }
}

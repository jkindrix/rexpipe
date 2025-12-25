// JavaScript file for testing all tree-sitter scopes

// IMPORT SCOPE - import statements
import { helper } from './utils';
import defaultExport from 'module';
const fs = require('fs');
const { readFile, writeFile } = require('fs/promises');

// FUNCTION SCOPE - function definitions
function helperFunction(x) {
    return x * 2;
}

const arrowHelper = (x) => x * 3;

async function asyncHelper(x) {
    return await Promise.resolve(x * 4);
}

// CLASS with methods
class User {
    constructor(name, age) {
        this.name = name;
        this.age = age;
    }

    greet() {
        // Comment about helper
        console.log(`Hello, ${this.name}`);
    }

    helperMethod() {
        return this.age * 2;
    }
}

// FUNCTION_CALLS SCOPE
const result = helperFunction(42);
const user = new User("Alice", 30);
user.greet();
console.log("Result:", result);

// CONTROL_FLOW SCOPE
function controlFlowExamples() {
    const x = 5;

    if (x > 0) {
        console.log("positive");
    } else if (x < 0) {
        console.log("negative");
    } else {
        console.log("zero");
    }

    for (let i = 0; i < 10; i++) {
        console.log(i);
    }

    while (x > 0) {
        break;
    }

    switch (x) {
        case 0:
            console.log("zero");
            break;
        default:
            console.log("other");
    }

    try {
        riskyOperation();
    } catch (e) {
        console.error(e);
    } finally {
        cleanup();
    }
}

// IDENTIFIERS SCOPE
const myVariable = 42;
let anotherVar = "hello";
var legacyVar = true;

// STRING SCOPE
const greeting = "hello in string";
const template = `hello ${name} in template`;
const multiline = `hello in
multiline template`;

// TESTS SCOPE - Jest/Mocha style
describe('User', () => {
    it('should greet correctly', () => {
        const user = new User('Test', 25);
        expect(user.name).toBe('Test');
    });

    test('helper method works', () => {
        const user = new User('Test', 10);
        expect(user.helperMethod()).toBe(20);
    });

    beforeEach(() => {
        // Setup code
    });

    afterEach(() => {
        // Cleanup code
    });
});

// More helper mentions
const helperConfig = { helper: true };

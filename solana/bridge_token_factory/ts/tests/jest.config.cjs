/** @type {import('ts-jest').JestConfigWithTsJest} */
module.exports = {
  preset: 'ts-jest',
  testEnvironment: 'node',
  testTimeout: 200000,
  setupFilesAfterEnv: ['<rootDir>/src/jestSetup.ts'],
};

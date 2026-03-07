# Security Policy

NEAR Omni Bridge is held to the highest security standard. This document defines the policy on how to report vulnerabilities and receive updates when security patches are released.

If you have any suggestions or comments about the security policy, please email the [NEAR Security Team](mailto:security@near.org) at security@near.org

## Reporting a vulnerability

All security issues should be submitted through our bug bounty program on [HackenProof](https://hackenproof.com/programs/near-intents-bridges). The team will review the submissions and decide whether they are eligible for bounty payouts. For more details, please check out the program description on the HackenProof website.

**Please do not open GitHub issues for security vulnerabilities.**

## Handling & disclosure process

1. Security report is received and assigned to an owner. This person will coordinate the process of evaluating, fixing, releasing and disclosing the issue.
2. After the initial report is received, the evaluation process is performed. It's identified if the issue exists, its severity and which version / components of the code is affected. Additional review to identify similar issues also happens.
3. Fixes are implemented for all supported releases. These fixes are not publicly communicated but held in a private repo of the Security Team or locally.
4. A suggested announcement date for this vulnerability is chosen. The notification is drafted and includes patches to all supported versions and affected components.
5. On the announcement date, the [NEAR Security Update newsletter](https://groups.google.com/a/near.org/g/security-updates) is sent an announcement. The changes are fast tracked and merged into the public repository. At least 6 hours after the mailing list is notified, a copy of the advisory will be published across social channels.

This process may take time, especially when coordinating with network participants and maintainers of other components in the ecosystem.
The goal will be to address issues in as short of a period as possible, but it's important that the process described above to ensure that disclosures are handled in a consistent manner.

*Note:* If the Security Team identifies that an issue is mission-critical and requires a subset of network participants to update prior to newsletter announcement - this will be done in a manual way by communicating via direct channels.

## In-scope vulnerabilities

The list is not limited to the following submissions but it gives an overview of what issues we care about:

* Stealing or loss of funds
* Unauthorized transactions
* Balance manipulation
* Contract execution flow issues
* Cryptographic flaws
* Unauthorized access to MPC key shares or signing capability
* Information disclosure of sensitive MPC state
* Bypass of threshold signature requirements
* Theft or permanent freezing of funds
* Cross-chain replay attacks enabling double-spending
* Light client verification bypass

## Out-of-scope vulnerabilities

* All vulnerabilities already discovered by audit reports
* Unbounded gas or storage consumption
* Griefing (e.g. no profit motive for an attacker, but damage to the users or the protocol)
* Network-level DoS
* Vulnerabilities in the protocol that are unrelated to smart contract execution
* Wormhole guardian network (report to Wormhole)
* Social engineering or physical attacks including physical attacks on TEE hardware
* Attacks requiring >= threshold colluding nodes
* NEAR chain attacks — validator collusion, chain reorgs, or finality failures
* Test-only code paths (e.g. code gated behind `#[cfg(test)]` or test utilities not reachable in production)
* Non-default feature-only paths (e.g. findings that depend on non-production feature gates)
* Deployment / operational issues — misconfigured infrastructure, key management procedures, etc.
* Decimal normalization rounding dust — `normalize_amount` uses integer (floor) division when bridging to chains with fewer decimals (e.g. 18-decimal ERC-20 to Solana's 9-decimal SPL tokens). The truncated remainder ("dust") is always less than one unit in the destination token's smallest denomination. When fee > 0, dust is absorbed into the fee recipient's payout via `claim_fee`. When fee = 0, dust remains locked in the contract (native tokens) or is effectively burned (bridged tokens). This is inherent to cross-chain decimal normalization
* Rejected relayer stake goes to DAO — In `reject_relayer_application` (`relayer_staking.rs`), the rejected applicant's staked NEAR is transferred to `env::predecessor_account_id()` (the DAO/RelayerManager caller), not back to the applicant. This is intentional: rejection is a punitive action for misbehaving or unqualified applicants, and the DAO retains the stake. Relayers who voluntarily leave use `resign_trusted_relayer`, which correctly returns the stake to the caller (who is the relayer themselves)

## Reward

| Severity | Reward |
|----------|--------|
| Critical | up to $100,000 |
| High     | up to $10,000 |
| Medium   | up to $5,000 |
| Low      | up to $1,000 |

The total maximum reward for High and Critical severity bugs is capped at 10% of the funds that are practically affected by the discovered vulnerability.

The following are the necessary conditions for the reward:
* You must be the first reporter of the vulnerability;
* The vulnerability must be reported no later than 24 hours after discovery and exclusively through [HackenProof](https://hackenproof.com/programs/near-intents-bridges);
* The vulnerability is not disclosed to anyone else except the finder and NEAR before it is fixed;
* The vulnerability is not exploited until it is fixed;
* You must provide a clear textual description of the report along with steps to reproduce the issue, including attachments such as screenshots or proof of concept code as necessary.

*Note:* The company is entitled to make the payment in their native NEAR token vested over 1 year.

## Receive Security Updates

If you want to be informed about security vulnerabilities, please subscribe to the [NEAR Security Update newsletter](https://groups.google.com/a/near.org/g/security-updates).
The newsletter is very low traffic and only sent out when public disclosure of a vulnerability happens.

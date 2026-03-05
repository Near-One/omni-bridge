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

## Reward

The discovery of security vulnerabilities that include but are not limited to the following categories will be rewarded proportionally to their severity:
* Issues that allow unauthorized or incorrect cross-chain asset transfers;
* Issues that allow theft or freezing of bridged funds;
* Algorithmic, implementation, and economic issues that violate safety of the bridge contracts;
* Issues in the MPC signature verification or proof validation logic;
* Issues that expose the private data of the users or the validators;
* Issues that stall the bridge or significantly throttle liveness;

The following are the necessary conditions for the reward:
* The vulnerability is disclosed to NEAR before it is disclosed publicly and NEAR is given sufficient time to fix it;
* The vulnerability is not disclosed to anyone else except the finder and NEAR before it is fixed;
* The vulnerability is not exploited until it is fixed.

## Receive Security Updates

If you want to be informed about security vulnerabilities, please subscribe to the [NEAR Security Update newsletter](https://groups.google.com/a/near.org/g/security-updates).
The newsletter is very low traffic and only sent out when public disclosure of a vulnerability happens.

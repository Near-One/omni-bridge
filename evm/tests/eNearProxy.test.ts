import type { HardhatEthersSigner } from "@nomicfoundation/hardhat-ethers/signers"
import { expect } from "chai"
import { ethers, upgrades } from "hardhat"
import type { ENearProxy, FakeProver, IENear } from "../typechain-types"

const UNPAUSED_ALL = 0
const PAUSE_FINALISE_FROM_NEAR = 1 << 0
const PAUSE_TRANSFER_TO_NEAR = 1 << 1

describe("eNearProxy contract", () => {
  let deployer: HardhatEthersSigner
  let eNearAdmin: HardhatEthersSigner
  let alice: HardhatEthersSigner
  let _bob: HardhatEthersSigner
  let nearProver: FakeProver
  let eNear: IENear
  let eNearProxy: ENearProxy

  const ERC20_NAME = "eNear"
  const ERC20_SYMBOL = "eNear"

  //https://etherscan.io/address/0x85F17Cf997934a597031b2E18a9aB6ebD4B9f6a4#code
  const eNearDeployBytecode =
    "0x60806040523480156200001157600080fd5b5060405162002d2638038062002d26833981810160405260e08110156200003757600080fd5b81019080805160405193929190846401000000008211156200005857600080fd5b9083019060208201858111156200006e57600080fd5b82516401000000008111828201881017156200008957600080fd5b82525081516020918201929091019080838360005b83811015620000b85781810151838201526020016200009e565b50505050905090810190601f168015620000e65780820380516001836020036101000a031916815260200191505b50604052602001805160405193929190846401000000008211156200010a57600080fd5b9083019060208201858111156200012057600080fd5b82516401000000008111828201881017156200013b57600080fd5b82525081516020918201929091019080838360005b838110156200016a57818101518382015260200162000150565b50505050905090810190601f168015620001985780820380516001836020036101000a031916815260200191505b5060405260200180516040519392919084640100000000821115620001bc57600080fd5b908301906020820185811115620001d257600080fd5b8251640100000000811182820188101715620001ed57600080fd5b82525081516020918201929091019080838360005b838110156200021c57818101518382015260200162000202565b50505050905090810190601f1680156200024a5780820380516001836020036101000a031916815260200191505b506040908152602082810151918301516060840151608090940151895193965090945091839183918791899188918d918d916200028d916003918501906200035a565b508051620002a39060049060208401906200035a565b505060058054601260ff1990911617610100600160a81b0319166101006001600160a01b03871602179055508151620002e49060069060208501906200035a565b50600780546001600160401b0319166001600160401b03929092169190911790555050600980546001600160a01b0319166001600160a01b039390931692909217909155600a5562000337601862000344565b50505050505050620003f6565b6005805460ff191660ff92909216919091179055565b828054600181600116156101000203166002900490600052602060002090601f016020900481019282601f106200039d57805160ff1916838001178555620003cd565b82800160010185558215620003cd579182015b82811115620003cd578251825591602001919060010190620003b0565b50620003db929150620003df565b5090565b5b80821115620003db5760008155600101620003e0565b61292080620004066000396000f3fe60806040526004361061014b5760003560e01c80636bf43296116100b6578063be831a2e1161006f578063be831a2e14610614578063c30a0f2514610644578063dd62ed3e1461066e578063e3113e3b146106a9578063f48ab4e014610763578063f851a4401461076b5761014b565b80636bf432961461047357806370a08231146104a457806395d89b41146104d7578063a457c2d7146104ec578063a9059cbb14610525578063b8e9744c1461055e5761014b565b8063313ce56711610108578063313ce567146102d257806332a8f30f146102fd578063395093511461032e5780633a239bee14610367578063530208f2146104255780635c975abb1461045e5761014b565b806306fdde0314610150578063095ea7b3146101da57806318160ddd1461022757806323b872dd1461024e5780632692c59f146102915780632a8853cd146102bd575b600080fd5b34801561015c57600080fd5b50610165610780565b6040805160208082528351818301528351919283929083019185019080838360005b8381101561019f578181015183820152602001610187565b50505050905090810190601f1680156101cc5780820380516001836020036101000a031916815260200191505b509250505060405180910390f35b3480156101e657600080fd5b50610213600480360360408110156101fd57600080fd5b506001600160a01b038135169060200135610816565b604080519115158252519081900360200190f35b34801561023357600080fd5b5061023c610833565b60408051918252519081900360200190f35b34801561025a57600080fd5b506102136004803603606081101561027157600080fd5b506001600160a01b03813581169160208101359091169060400135610839565b34801561029d57600080fd5b506102bb600480360360208110156102b457600080fd5b50356108c0565b005b3480156102c957600080fd5b506101656108dc565b3480156102de57600080fd5b506102e761096a565b6040805160ff9092168252519081900360200190f35b34801561030957600080fd5b50610312610973565b604080516001600160a01b039092168252519081900360200190f35b34801561033a57600080fd5b506102136004803603604081101561035157600080fd5b506001600160a01b038135169060200135610987565b34801561037357600080fd5b506102bb6004803603604081101561038a57600080fd5b8101906020810181356401000000008111156103a557600080fd5b8201836020820111156103b757600080fd5b803590602001918460018302840111640100000000831117156103d957600080fd5b91908080601f016020809104026020016040519081016040528093929190818152602001838380828437600092019190915250929550505090356001600160401b031691506109d59050565b34801561043157600080fd5b506102bb6004803603604081101561044857600080fd5b506001600160a01b038135169060200135610a9c565b34801561046a57600080fd5b5061023c610aee565b34801561047f57600080fd5b50610488610af4565b604080516001600160401b039092168252519081900360200190f35b3480156104b057600080fd5b5061023c600480360360208110156104c757600080fd5b50356001600160a01b0316610b03565b3480156104e357600080fd5b50610165610b22565b3480156104f857600080fd5b506102136004803603604081101561050f57600080fd5b506001600160a01b038135169060200135610b83565b34801561053157600080fd5b506102136004803603604081101561054857600080fd5b506001600160a01b038135169060200135610beb565b6101656004803603604081101561057457600080fd5b6001600160a01b03823516919081019060408101602082013564010000000081111561059f57600080fd5b8201836020820111156105b157600080fd5b803590602001918460018302840111640100000000831117156105d357600080fd5b91908080601f016020809104026020016040519081016040528093929190818152602001838380828437600092019190915250929550610bff945050505050565b34801561062057600080fd5b506102bb6004803603604081101561063757600080fd5b5080359060200135610cd3565b34801561065057600080fd5b506102136004803603602081101561066757600080fd5b5035610cee565b34801561067a57600080fd5b5061023c6004803603604081101561069157600080fd5b506001600160a01b0381358116916020013516610d03565b3480156106b557600080fd5b506102bb600480360360408110156106cc57600080fd5b813591908101906040810160208201356401000000008111156106ee57600080fd5b82018360208201111561070057600080fd5b8035906020019184600183028401116401000000008311171561072257600080fd5b91908080601f016020809104026020016040519081016040528093929190818152602001838380828437600092019190915250929550610d2e945050505050565b6102bb610e10565b34801561077757600080fd5b50610312610e29565b60038054604080516020601f600260001961010060018816150201909516949094049384018190048102820181019092528281526060939092909183018282801561080c5780601f106107e15761010080835404028352916020019161080c565b820191906000526020600020905b8154815290600101906020018083116107ef57829003601f168201915b5050505050905090565b600061082a610823610e38565b8484610e3c565b50600192915050565b60025490565b6000610846848484610f28565b6108b684610852610e38565b6108b18560405180606001604052806028815260200161276a602891396001600160a01b038a16600090815260016020526040812090610890610e38565b6001600160a01b031681526020810191909152604001600020549190611083565b610e3c565b5060019392505050565b6009546001600160a01b031633146108d757600080fd5b600a55565b6006805460408051602060026001851615610100026000190190941693909304601f810184900484028201840190925281815292918301828280156109625780601f1061093757610100808354040283529160200191610962565b820191906000526020600020905b81548152906001019060200180831161094557829003601f168201915b505050505081565b60055460ff1690565b60055461010090046001600160a01b031681565b600061082a610994610e38565b846108b185600160006109a5610e38565b6001600160a01b03908116825260208083019390935260409182016000908120918c16815292529020549061111a565b600180600a5416600014806109f457506009546001600160a01b031633145b6109fd57600080fd5b610a0561244b565b610a0f848461117b565b9050610a19612478565b610a2682606001516115b6565b9050610a43816020015182600001516001600160801b0316611664565b80602001516001600160a01b03167f3538c3349544a9ce6d1cfda849857b2b8fa919c15fe6d382e08573b9838d2aa8826000015160405180826001600160801b0316815260200191505060405180910390a25050505050565b6009546001600160a01b03163314610ab357600080fd5b6040516001600160a01b0383169082156108fc029083906000818181858888f19350505050158015610ae9573d6000803e3d6000fd5b505050565b600a5481565b6007546001600160401b031681565b6001600160a01b0381166000908152602081905260409020545b919050565b60048054604080516020601f600260001961010060018816150201909516949094049384018190048102820181019092528281526060939092909183018282801561080c5780601f106107e15761010080835404028352916020019161080c565b600061082a610b90610e38565b846108b1856040518060600160405280602581526020016128c66025913960016000610bba610e38565b6001600160a01b03908116825260208083019390935260409182016000908120918d16815292529020549190611083565b600061082a610bf8610e38565b8484610f28565b6009546060906001600160a01b03163314610c1957600080fd5b60006060846001600160a01b0316846040518082805190602001908083835b60208310610c575780518252601f199092019160209182019101610c38565b6001836020036101000a038019825116818451168082178552505050505050905001915050600060405180830381855af49150503d8060008114610cb7576040519150601f19603f3d011682016040523d82523d6000602084013e610cbc565b606091505b509150915081610ccb57600080fd5b949350505050565b6009546001600160a01b03163314610cea57600080fd5b9055565b60086020526000908152604090205460ff1681565b6001600160a01b03918216600090815260016020908152604080832093909416825291909152205490565b600280600a541660001480610d4d57506009546001600160a01b031633145b610d5657600080fd5b610d603384611754565b336001600160a01b03167fabeef16c62fe7504587dd9ef5d707aeb0932570da8eb1a4f099c6e80524b17c384846040518083815260200180602001828103825283818151815260200191508051906020019080838360005b83811015610dd0578181015183820152602001610db8565b50505050905090810190601f168015610dfd5780820380516001836020036101000a031916815260200191505b50935050505060405180910390a2505050565b6009546001600160a01b03163314610e2757600080fd5b565b6009546001600160a01b031681565b3390565b6001600160a01b038316610e815760405162461bcd60e51b81526004018080602001828103825260248152602001806128656024913960400191505060405180910390fd5b6001600160a01b038216610ec65760405162461bcd60e51b81526004018080602001828103825260228152602001806126b66022913960400191505060405180910390fd5b6001600160a01b03808416600081815260016020908152604080832094871680845294825291829020859055815185815291517f8c5be1e5ebec7d5bd14f71427d1e84f3dd0314c0f7b2291e5b200ac8c7c3b9259281900390910190a3505050565b6001600160a01b038316610f6d5760405162461bcd60e51b81526004018080602001828103825260258152602001806127ef6025913960400191505060405180910390fd5b6001600160a01b038216610fb25760405162461bcd60e51b81526004018080602001828103825260238152602001806126296023913960400191505060405180910390fd5b610fbd838383610ae9565b610ffa8160405180606001604052806026815260200161270f602691396001600160a01b0386166000908152602081905260409020549190611083565b6001600160a01b038085166000908152602081905260408082209390935590841681522054611029908261111a565b6001600160a01b038084166000818152602081815260409182902094909455805185815290519193928716927fddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef92918290030190a3505050565b600081848411156111125760405162461bcd60e51b81526004018080602001828103825283818151815260200191508051906020019080838360005b838110156110d75781810151838201526020016110bf565b50505050905090810190601f1680156111045780820380516001836020036101000a031916815260200191505b509250505060405180910390fd5b505050900390565b600082820183811015611174576040805162461bcd60e51b815260206004820152601b60248201527f536166654d6174683a206164646974696f6e206f766572666c6f770000000000604482015290519081900360640190fd5b9392505050565b61118361244b565b600554604080516392d68dfd60e01b81526001600160401b0385166024820152600481019182528551604482015285516101009093046001600160a01b0316926392d68dfd9287928792829160640190602086019080838360005b838110156111f65781810151838201526020016111de565b50505050905090810190601f1680156112235780820380516001836020036101000a031916815260200191505b50935050505060206040518083038186803b15801561124157600080fd5b505afa158015611255573d6000803e3d6000fd5b505050506040513d602081101561126b57600080fd5b50516112b6576040805162461bcd60e51b8152602060048201526015602482015274141c9bdbd9881cda1bdd5b19081899481d985b1a59605a1b604482015290519081900360640190fd5b6112be61248f565b6112c784611850565b90506112d16124a9565b6112da82611872565b6007546040808301510151519192506001600160401b0390811691161015611349576040805162461bcd60e51b815260206004820152601f60248201527f50726f6f662069732066726f6d2074686520616e6369656e7420626c6f636b00604482015290519081900360640190fd5b611352826118b4565b61138d5760405162461bcd60e51b815260040180806020018281038252602c815260200180612814602c913960400191505060405180910390fd5b600081600001516040015160200151602001516000815181106113ac57fe5b6020908102919091018101516000818152600890925260409091205490915060ff161561140a5760405162461bcd60e51b81526004018080602001828103825260258152602001806128406025913960400191505060405180910390fd5b60008181526008602052604090819020805460ff19166001908117909155905160068054909282918491600260001991831615610100029190910190911604801561148c5780601f1061146a57610100808354040283529182019161148c565b820191906000526020600020905b815481529060010190602001808311611478575b50509150506040518091039020826000015160400151602001516080015180519060200120146114ed5760405162461bcd60e51b815260040180806020018281038252604881526020018061264c6048913960600191505060405180910390fd5b8160000151604001516020015160a0015193508360400151156115415760405162461bcd60e51b815260040180806020018281038252603c815260200180612792603c913960400191505060405180910390fd5b8360200151156115825760405162461bcd60e51b815260040180806020018281038252603d815260200180612889603d913960400191505060405180910390fd5b60405181907fb226e263cb7a3bde6afd6e46c543e956d49171b4fe4f0daf93cb1798f2315d1d90600090a250505092915050565b6115be612478565b6115c661248f565b6115cf83611850565b905060006115dc826118c0565b905060ff811615611634576040805162461bcd60e51b815260206004820152601760248201527f4552525f4e4f545f57495448445241575f524553554c54000000000000000000604482015290519081900360640190fd5b61163d82611942565b6001600160801b03168352600061165383611974565b60601c602085015250919392505050565b6001600160a01b0382166116bf576040805162461bcd60e51b815260206004820152601f60248201527f45524332303a206d696e7420746f20746865207a65726f206164647265737300604482015290519081900360640190fd5b6116cb60008383610ae9565b6002546116d8908261111a565b6002556001600160a01b0382166000908152602081905260409020546116fe908261111a565b6001600160a01b0383166000818152602081815260408083209490945583518581529351929391927fddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef9281900390910190a35050565b6001600160a01b0382166117995760405162461bcd60e51b81526004018080602001828103825260218152602001806127ce6021913960400191505060405180910390fd5b6117a582600083610ae9565b6117e281604051806060016040528060228152602001612694602291396001600160a01b0385166000908152602081905260409020549190611083565b6001600160a01b03831660009081526020819052604090205560025461180890826119b0565b6002556040805182815290516000916001600160a01b038516917fddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef9181900360200190a35050565b61185861248f565b506040805180820190915260008152602081019190915290565b61187a6124a9565b61188382611a0d565b815261188e82611a41565b602082015261189c82611ae1565b60408201526118aa82611a41565b6060820152919050565b60208101515190511490565b600081600180826000015101826020015151101561191b576040805162461bcd60e51b8152602060048201526013602482015272426f7273683a204f7574206f662072616e676560681b604482015290519081900360640190fd5b602084015184518151811061192c57fe5b0160200151825190910190915260f81c92915050565b600061194d82611c86565b6001600160401b03169050604061196383611c86565b6001600160401b0316901b17919050565b6000805b60148110156119aa578060080261198e846118c0565b60f81b6001600160f81b031916901c9190911790600101611978565b50919050565b600082821115611a07576040805162461bcd60e51b815260206004820152601e60248201527f536166654d6174683a207375627472616374696f6e206f766572666c6f770000604482015290519081900360640190fd5b50900390565b611a156124e8565b611a1e82611a41565b8152611a2982611cb2565b6020820152611a3782611d27565b6040820152919050565b611a4961250f565b611a5282611ea0565b63ffffffff166001600160401b0381118015611a6d57600080fd5b50604051908082528060200260200182016040528015611aa757816020015b611a94612478565b815260200190600190039081611a8c5790505b50815260005b8151518110156119aa57611ac083611ec8565b8251805183908110611ace57fe5b6020908102919091010152600101611aad565b611ae9612522565b611af282611cb2565b8152611afd82611cb2565b6020820152611b0b82611f2c565b816040018190525060028082604001516101000151836020015160405160200180838152602001828152602001925050506040516020818303038152906040526040518082805190602001908083835b60208310611b7a5780518252601f199092019160209182019101611b5b565b51815160209384036101000a60001901801990921691161790526040519190930194509192505080830381855afa158015611bb9573d6000803e3d6000fd5b5050506040513d6020811015611bce57600080fd5b50518251604080516020818101949094528082019290925280518083038201815260609092019081905281519192909182918401908083835b60208310611c265780518252601f199092019160209182019101611c07565b51815160209384036101000a60001901801990921691161790526040519190930194509192505080830381855afa158015611c65573d6000803e3d6000fd5b5050506040513d6020811015611c7a57600080fd5b50516060820152919050565b6000611c9182611ea0565b63ffffffff1690506020611ca483611ea0565b63ffffffff16901b17919050565b6000816020808260000151018260200151511015611d0d576040805162461bcd60e51b8152602060048201526013602482015272426f7273683a204f7574206f662072616e676560681b604482015290519081900360640190fd5b602080850151945190940190930151815190930190525090565b611d2f61254e565b611d3882611cb2565b8152611d4382611fc9565b81602001819052506000816020015160c001515160010190506002600082901c60ff16600883901c60ff16601084901c60ff16601885901c60ff168660000151876020015160c00151604051602001808760ff1660f81b81526001018660ff1660f81b81526001018560ff1660f81b81526001018460ff1660f81b8152600101838152602001828051906020019060200280838360005b83811015611df2578181015183820152602001611dda565b5050505090500196505050505050506040516020818303038152906040526040518082805190602001908083835b60208310611e3f5780518252601f199092019160209182019101611e20565b51815160209384036101000a60001901801990921691161790526040519190930194509192505080830381855afa158015611e7e573d6000803e3d6000fd5b5050506040513d6020811015611e9357600080fd5b5051604083015250919050565b6000611eab82612297565b61ffff1690506010611ebc83612297565b61ffff16901b17919050565b611ed0612478565b611ed982611cb2565b8152611ee4826118c0565b60ff1660208201819052600211610b1d5760405162461bcd60e51b81526004018080602001828103825260378152602001806126d86037913960400191505060405180910390fd5b611f34612569565b611f3f8260d06122bd565b610100820152611f4e82611c86565b6001600160401b03168152611f6282611cb2565b6020820152611f7082611cb2565b6040820152611f7e82611cb2565b6060820152611f8c82611cb2565b6080820152611f9a82611c86565b6001600160401b031660a0820152611fb182611cb2565b60c0820152611fbf82611cb2565b60e0820152919050565b611fd16125b5565b611fda82611ea0565b63ffffffff166001600160401b0381118015611ff557600080fd5b5060405190808252806020026020018201604052801561202957816020015b60608152602001906001900390816120145790505b50815260005b81515181101561206357612042836122d2565b825180518390811061205057fe5b602090810291909101015260010161202f565b50815161206f83611ea0565b63ffffffff166001600160401b038111801561208a57600080fd5b506040519080825280602002602001820160405280156120b4578160200160208202803683370190505b50602083015260005b8260200151518110156120f6576120d384611cb2565b836020015182815181106120e357fe5b60209081029190910101526001016120bd565b5061210083611c86565b6001600160401b0316604083015261211783611942565b6001600160801b0316606083015261212e836122d2565b608083015261213c83612369565b60a083015282518251516001016001600160401b038111801561215e57600080fd5b50604051908082528060200260200182016040528015612188578160200160208202803683370190505b5060c084015281845261219d848383036122bd565b8360c001516000815181106121ae57fe5b602090810291909101015280845260005b83515181101561228f576002846000015182815181106121db57fe5b60200260200101516040518082805190602001908083835b602083106122125780518252601f1990920191602091820191016121f3565b51815160209384036101000a60001901801990921691161790526040519190930194509192505080830381855afa158015612251573d6000803e3d6000fd5b5050506040513d602081101561226657600080fd5b505160c085015180516001840190811061227c57fe5b60209081029190910101526001016121bf565b505050919050565b60006122a2826118c0565b60ff16905060086122b2836118c0565b60ff16901b17919050565b60006111748360200151846000015184612429565b60606122dd82611ea0565b63ffffffff166001600160401b03811180156122f857600080fd5b506040519080825280601f01601f191660200182016040528015612323576020820181803683370190505b50905060005b81518110156119aa5761233b836118c0565b60f81b82828151811061234a57fe5b60200101906001600160f81b031916908160001a905350600101612329565b61237161244b565b61237a826118c0565b60ff168082526123905760016020820152610b1d565b806000015160ff16600114156123ac5760016040820152610b1d565b806000015160ff16600214156123cf576123c5826122d2565b6060820152610b1d565b806000015160ff16600314156123f2576123e882611cb2565b6080820152610b1d565b60405162461bcd60e51b81526004018080602001828103825260358152602001806127356035913960400191505060405180910390fd5b600061243361260a565b6020818486602089010160025afa5051949350505050565b6040805160a081018252600080825260208201819052918101829052606080820152608081019190915290565b604080518082019091526000808252602082015290565b604051806040016040528060008152602001606081525090565b60405180608001604052806124bc6124e8565b81526020016124c961250f565b81526020016124d6612522565b81526020016124e361250f565b905290565b60405180606001604052806124fb61250f565b8152600060208201526040016124e361254e565b6040518060200160405280606081525090565b6040805160808101825260008082526020820152908101612541612569565b8152600060209091015290565b604080516060810190915260008152602081016125416125b5565b6040805161012081018252600080825260208201819052918101829052606081018290526080810182905260a0810182905260c0810182905260e0810182905261010081019190915290565b6040518060e00160405280606081526020016060815260200160006001600160401b0316815260200160006001600160801b03168152602001606081526020016125fd61244b565b8152602001606081525090565b6040518060200160405280600190602082028036833750919291505056fe45524332303a207472616e7366657220746f20746865207a65726f206164647265737343616e206f6e6c7920756e6c6f636b20746f6b656e732066726f6d20746865206c696e6b65642070726f6f662070726f6475636572206f6e204e65617220626c6f636b636861696e45524332303a206275726e20616d6f756e7420657863656564732062616c616e636545524332303a20617070726f766520746f20746865207a65726f206164647265737350726f6f664465636f6465723a204d65726b6c65506174684974656d20646972656374696f6e2073686f756c642062652030206f72203145524332303a207472616e7366657220616d6f756e7420657863656564732062616c616e63654e6561724465636f6465723a206465636f6465457865637574696f6e53746174757320696e646578206f7574206f662072616e676545524332303a207472616e7366657220616d6f756e74206578636565647320616c6c6f77616e636543616e6e6f7420757365206661696c656420657865637574696f6e206f7574636f6d6520666f7220756e6c6f636b696e672074686520746f6b656e7345524332303a206275726e2066726f6d20746865207a65726f206164647265737345524332303a207472616e736665722066726f6d20746865207a65726f2061646472657373417267756d656e742073686f756c6420626520657861637420626f7273682073657269616c697a6174696f6e546865206275726e206576656e742070726f6f662063616e6e6f742062652072657573656445524332303a20617070726f76652066726f6d20746865207a65726f206164647265737343616e6e6f742075736520756e6b6e6f776e20657865637574696f6e206f7574636f6d6520666f7220756e6c6f636b696e672074686520746f6b656e7345524332303a2064656372656173656420616c6c6f77616e63652062656c6f77207a65726fa26469706673582212201225bbd7f0a82122ab1cb419399486bffa69ce7288e9343b357e554f3a55808a64736f6c634300060c0033"
  const eNearAbi =
    '[{"inputs":[{"internalType":"string","name":"_tokenName","type":"string"},{"internalType":"string","name":"_tokenSymbol","type":"string"},{"internalType":"bytes","name":"_nearConnector","type":"bytes"},{"internalType":"contract INearProver","name":"_prover","type":"address"},{"internalType":"uint64","name":"_minBlockAcceptanceHeight","type":"uint64"},{"internalType":"address","name":"_admin","type":"address"},{"internalType":"uint256","name":"_pausedFlags","type":"uint256"}],"stateMutability":"nonpayable","type":"constructor"},{"anonymous":false,"inputs":[{"indexed":true,"internalType":"address","name":"owner","type":"address"},{"indexed":true,"internalType":"address","name":"spender","type":"address"},{"indexed":false,"internalType":"uint256","name":"value","type":"uint256"}],"name":"Approval","type":"event"},{"anonymous":false,"inputs":[{"indexed":true,"internalType":"bytes32","name":"_receiptId","type":"bytes32"}],"name":"ConsumedProof","type":"event"},{"anonymous":false,"inputs":[{"indexed":false,"internalType":"uint128","name":"amount","type":"uint128"},{"indexed":true,"internalType":"address","name":"recipient","type":"address"}],"name":"NearToEthTransferFinalised","type":"event"},{"anonymous":false,"inputs":[{"indexed":true,"internalType":"address","name":"from","type":"address"},{"indexed":true,"internalType":"address","name":"to","type":"address"},{"indexed":false,"internalType":"uint256","name":"value","type":"uint256"}],"name":"Transfer","type":"event"},{"anonymous":false,"inputs":[{"indexed":true,"internalType":"address","name":"sender","type":"address"},{"indexed":false,"internalType":"uint256","name":"amount","type":"uint256"},{"indexed":false,"internalType":"string","name":"accountId","type":"string"}],"name":"TransferToNearInitiated","type":"event"},{"inputs":[],"name":"admin","outputs":[{"internalType":"address","name":"","type":"address"}],"stateMutability":"view","type":"function"},{"inputs":[{"internalType":"address","name":"target","type":"address"},{"internalType":"bytes","name":"data","type":"bytes"}],"name":"adminDelegatecall","outputs":[{"internalType":"bytes","name":"","type":"bytes"}],"stateMutability":"payable","type":"function"},{"inputs":[{"internalType":"uint256","name":"flags","type":"uint256"}],"name":"adminPause","outputs":[],"stateMutability":"nonpayable","type":"function"},{"inputs":[],"name":"adminReceiveEth","outputs":[],"stateMutability":"payable","type":"function"},{"inputs":[{"internalType":"address payable","name":"destination","type":"address"},{"internalType":"uint256","name":"amount","type":"uint256"}],"name":"adminSendEth","outputs":[],"stateMutability":"nonpayable","type":"function"},{"inputs":[{"internalType":"uint256","name":"key","type":"uint256"},{"internalType":"uint256","name":"value","type":"uint256"}],"name":"adminSstore","outputs":[],"stateMutability":"nonpayable","type":"function"},{"inputs":[{"internalType":"address","name":"owner","type":"address"},{"internalType":"address","name":"spender","type":"address"}],"name":"allowance","outputs":[{"internalType":"uint256","name":"","type":"uint256"}],"stateMutability":"view","type":"function"},{"inputs":[{"internalType":"address","name":"spender","type":"address"},{"internalType":"uint256","name":"amount","type":"uint256"}],"name":"approve","outputs":[{"internalType":"bool","name":"","type":"bool"}],"stateMutability":"nonpayable","type":"function"},{"inputs":[{"internalType":"address","name":"account","type":"address"}],"name":"balanceOf","outputs":[{"internalType":"uint256","name":"","type":"uint256"}],"stateMutability":"view","type":"function"},{"inputs":[],"name":"decimals","outputs":[{"internalType":"uint8","name":"","type":"uint8"}],"stateMutability":"view","type":"function"},{"inputs":[{"internalType":"address","name":"spender","type":"address"},{"internalType":"uint256","name":"subtractedValue","type":"uint256"}],"name":"decreaseAllowance","outputs":[{"internalType":"bool","name":"","type":"bool"}],"stateMutability":"nonpayable","type":"function"},{"inputs":[{"internalType":"bytes","name":"proofData","type":"bytes"},{"internalType":"uint64","name":"proofBlockHeight","type":"uint64"}],"name":"finaliseNearToEthTransfer","outputs":[],"stateMutability":"nonpayable","type":"function"},{"inputs":[{"internalType":"address","name":"spender","type":"address"},{"internalType":"uint256","name":"addedValue","type":"uint256"}],"name":"increaseAllowance","outputs":[{"internalType":"bool","name":"","type":"bool"}],"stateMutability":"nonpayable","type":"function"},{"inputs":[],"name":"minBlockAcceptanceHeight","outputs":[{"internalType":"uint64","name":"","type":"uint64"}],"stateMutability":"view","type":"function"},{"inputs":[],"name":"name","outputs":[{"internalType":"string","name":"","type":"string"}],"stateMutability":"view","type":"function"},{"inputs":[],"name":"nearConnector","outputs":[{"internalType":"bytes","name":"","type":"bytes"}],"stateMutability":"view","type":"function"},{"inputs":[],"name":"paused","outputs":[{"internalType":"uint256","name":"","type":"uint256"}],"stateMutability":"view","type":"function"},{"inputs":[],"name":"prover","outputs":[{"internalType":"contract INearProver","name":"","type":"address"}],"stateMutability":"view","type":"function"},{"inputs":[],"name":"symbol","outputs":[{"internalType":"string","name":"","type":"string"}],"stateMutability":"view","type":"function"},{"inputs":[],"name":"totalSupply","outputs":[{"internalType":"uint256","name":"","type":"uint256"}],"stateMutability":"view","type":"function"},{"inputs":[{"internalType":"address","name":"recipient","type":"address"},{"internalType":"uint256","name":"amount","type":"uint256"}],"name":"transfer","outputs":[{"internalType":"bool","name":"","type":"bool"}],"stateMutability":"nonpayable","type":"function"},{"inputs":[{"internalType":"address","name":"sender","type":"address"},{"internalType":"address","name":"recipient","type":"address"},{"internalType":"uint256","name":"amount","type":"uint256"}],"name":"transferFrom","outputs":[{"internalType":"bool","name":"","type":"bool"}],"stateMutability":"nonpayable","type":"function"},{"inputs":[{"internalType":"uint256","name":"_amount","type":"uint256"},{"internalType":"string","name":"_nearReceiverAccountId","type":"string"}],"name":"transferToNear","outputs":[],"stateMutability":"nonpayable","type":"function"},{"inputs":[{"internalType":"bytes32","name":"","type":"bytes32"}],"name":"usedProofs","outputs":[{"internalType":"bool","name":"","type":"bool"}],"stateMutability":"view","type":"function"}]'

  beforeEach(async () => {
    ;[deployer, eNearAdmin, alice, _bob] = await ethers.getSigners()

    const nearProverMockContractFactory = await ethers.getContractFactory("FakeProver")
    nearProver = await nearProverMockContractFactory.deploy()
    await nearProver.waitForDeployment()

    // Proofs coming from blocks below this value should be rejected
    const minBlockAcceptanceHeight = 0

    const eNearContractFactory = new ethers.ContractFactory(eNearAbi, eNearDeployBytecode, deployer)

    eNear = (await eNearContractFactory.deploy(
      ERC20_NAME,
      ERC20_SYMBOL,
      Buffer.from("eNearBridge", "utf-8"),
      await nearProver.getAddress(),
      minBlockAcceptanceHeight,
      eNearAdmin.address,
      UNPAUSED_ALL,
    )) as IENear

    await eNear.waitForDeployment()

    const eNearProxyFactory = await ethers.getContractFactory("ENearProxy")
    eNearProxy = (await upgrades.deployProxy(
      eNearProxyFactory,
      [
        await eNear.getAddress(),
        await nearProver.getAddress(),
        Buffer.from("eNearBridge", "utf-8"),
        0,
      ],
      { initializer: "initialize" },
    )) as unknown as ENearProxy

    await eNearProxy.waitForDeployment()
  })

  describe("transferToNear()", () => {
    it("Mint by using eNearProxy", async () => {
      await eNearProxy
        .connect(deployer)
        .grantRole(ethers.keccak256(ethers.toUtf8Bytes("MINTER_ROLE")), alice.address)
      await eNearProxy.connect(alice).mint(await eNear.getAddress(), alice.address, 100)
      expect(await eNear.balanceOf(alice.address)).to.equal(100)
    })

    it("Two mints by using eNearProxy", async () => {
      await eNearProxy
        .connect(deployer)
        .grantRole(ethers.keccak256(ethers.toUtf8Bytes("MINTER_ROLE")), alice.address)
      await eNearProxy.connect(alice).mint(await eNear.getAddress(), alice.address, 100)
      expect(await eNear.balanceOf(alice.address)).to.equal(100)

      await eNearProxy.connect(alice).mint(await eNear.getAddress(), alice.address, 100)
      expect(await eNear.balanceOf(alice.address)).to.equal(200)
    })

    it("Burn by using eNearProxy", async () => {
      await eNearProxy
        .connect(deployer)
        .grantRole(ethers.keccak256(ethers.toUtf8Bytes("MINTER_ROLE")), alice.address)
      await eNearProxy.connect(alice).mint(await eNear.getAddress(), alice.address, 100)
      expect(await eNear.balanceOf(alice.address)).to.equal(100)
      expect(await eNear.totalSupply()).to.equal(100)

      await eNear.connect(alice).transfer(await eNearProxy.getAddress(), 100)

      expect(await eNear.totalSupply()).to.equal(100)

      await eNearProxy.connect(alice).burn(await eNear.getAddress(), 100)

      expect(await eNear.totalSupply()).to.equal(0)
    })

    it("Set Fake Prover", async () => {
      const nearProverMockContractFactory = await ethers.getContractFactory("FakeProver")
      const fakeProver = await nearProverMockContractFactory.deploy()
      await fakeProver.waitForDeployment()

      expect(await eNear.prover()).to.equal(await nearProver.getAddress())
      let slotValue = await ethers.provider.getStorage(await eNear.getAddress(), 5)
      slotValue = (await fakeProver.getAddress()).concat(slotValue.slice(-2))

      await eNear.connect(eNearAdmin).adminSstore(5, ethers.zeroPadValue(slotValue, 32))
      expect(await eNear.prover()).to.equal(await fakeProver.getAddress())
    })

    it("Set Proxy as Admin", async () => {
      expect(await eNear.admin()).to.equal(await eNearAdmin.getAddress())
      await eNear
        .connect(eNearAdmin)
        .adminSstore(9, ethers.zeroPadValue(await eNearProxy.getAddress(), 32))
      expect(await eNear.admin()).to.equal(await eNearProxy.getAddress())
    })

    it("Pause All", async () => {
      await eNear.connect(eNearAdmin).adminPause(PAUSE_TRANSFER_TO_NEAR | PAUSE_FINALISE_FROM_NEAR)

      await eNearProxy
        .connect(deployer)
        .grantRole(ethers.keccak256(ethers.toUtf8Bytes("MINTER_ROLE")), alice.address)
      await expect(eNearProxy.connect(alice).mint(await eNear.getAddress(), alice.address, 100)).to
        .be.reverted
      expect(await eNear.balanceOf(alice.address)).to.equal(0)

      await eNear
        .connect(eNearAdmin)
        .adminSstore(9, ethers.zeroPadValue(await eNearProxy.getAddress(), 32))
      await eNearProxy.connect(alice).mint(await eNear.getAddress(), alice.address, 100)
      expect(await eNear.balanceOf(alice.address)).to.equal(100)
    })
  })
})

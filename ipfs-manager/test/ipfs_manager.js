const ipfsManager = artifacts.require("IpfsManager");

/*
 * uncomment accounts to access the test accounts made available by the
 * Ethereum client
 * See docs: https://www.trufflesuite.com/docs/truffle/testing/writing-tests-in-javascript
 */
contract("IpfsManager", function (accounts) {
  let BN = web3.utils.BN;
  var contract = null
  before("should assert true", async function () {
    contract = await ipfsManager.deployed();
    return assert.isTrue(true);
  });

  it("should get correct block price", async function () {
    let fee = await contract.get_per_block_price();
    return assert.equal(fee.toString(), web3.utils.toWei("100", "gwei").toString())
  });

  it("should update block price", async function () {
    await contract.update_per_block_price(web3.utils.toWei("999", "gwei"), {from: accounts[0]});
    let fee = await contract.get_per_block_price();
    return assert.equal(fee.toString(), web3.utils.toWei("999", "gwei").toString())
  });

  it("should calculate correct number of blocks", async function () {
    await contract.update_per_block_price(web3.utils.toWei("9", "gwei"), {from: accounts[0]});
    let num_b = await contract.get_num_of_valid_blocks(web3.utils.toWei("90", "gwei"));
    return assert.equal(num_b.toString(), new BN(web3.utils.toWei("90", "gwei").toString())/
                                          new BN(web3.utils.toWei("9", "gwei").toString()))
  });

  it("should add new valid blocks", async function () {
    await contract.update_per_block_price(web3.utils.toWei("9", "gwei"), {from: accounts[0]});
    let valid_b = await contract.get_valid_block("QmdDURevega5bLgTWkAnBvct2MDp2X2zpWDuYWv5f5oCZj");
    let log = await contract.add_new_valid_block("QmdDURevega5bLgTWkAnBvct2MDp2X2zpWDuYWv5f5oCZj",
                                                  {from: accounts[0], 
                                                   value: web3.utils.toWei("90", "gwei").toString()});
    for(let l of log.logs){
      if (l.event === "UpdateValidBlock"){
        return assert.isTrue(l.args.end_block.gt(valid_b));
      }
    }

    valid_b = await contract.get_valid_block("QmdDURevega5bLgTWkAnBvct2MDp2X2zpWDuYWv5f5oCZj");
    log = await contract.add_new_valid_block("QmdDURevega5bLgTWkAnBvct2MDp2X2zpWDuYWv5f5oCZj",
                                                  {from: accounts[0], 
                                                   value: web3.utils.toWei("90", "gwei").toString()});
    
    for(let l of log.logs){
        if (l.event === "UpdateValidBlock"){
          return assert.isTrue(l.args.end_block.eq(valid_b.add(new BN("10"))));
        }
      }                                        
    return assert.isTrue(false);
  });

});

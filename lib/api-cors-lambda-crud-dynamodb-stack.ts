import * as cdk from 'aws-cdk-lib';
import { LambdaRestApi } from 'aws-cdk-lib/aws-apigateway';
import { AttributeType } from 'aws-cdk-lib/aws-dynamodb';
import { Architecture, Runtime } from 'aws-cdk-lib/aws-lambda';
import { Construct } from 'constructs';

export class ApiCorsLambdaCrudDynamodbStack extends cdk.Stack {
  constructor(scope: Construct, id: string, props?: cdk.StackProps) {
    super(scope, id, props);

    const dynamoTable = new cdk.aws_dynamodb.Table(this, 'items', {
      partitionKey: {
        name: 'itemId',
        type: AttributeType.STRING
      },
      tableName: 'items',
      removalPolicy: cdk.RemovalPolicy.DESTROY
    });

    const itemsLambda = new cdk.aws_lambda.Function(this, 'itemsLambda', {
      runtime: Runtime.PROVIDED_AL2023,
      architecture: Architecture.ARM_64,
      code: cdk.aws_lambda.Code.fromAsset('lambda/target/lambda/crud-lambda'),
      handler: 'not.required',
      environment: {
        PK: 'itemId',
        TABLE_NAME: dynamoTable.tableName
      }
    });

    dynamoTable.grantReadWriteData(itemsLambda);

    new LambdaRestApi(this, 'itemsApi', {
      handler: itemsLambda
    });

  }
}
